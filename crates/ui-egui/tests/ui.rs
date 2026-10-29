//! UI tests, in the style of the other Craft apps: drive the real window headlessly, assert on
//! behaviour and on pixels, and never commit golden screenshots. Set `NOTECRAFT_UI_VISUAL_DIR`
//! to also write review pictures of key screens to that folder (see `docs/testing.md`).

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use notecraft_engine::sample::{Sample, create_sample};
use notecraft_engine::{Output, Params, Session};
use notecraft_store::Settings;
use notecraft_store::testing::TempDir;
use notecraft_ui_egui::control::{ControlRequest, start};
use notecraft_ui_egui::theme::{Accent, Mode, ModePref, Theme, contrast};
use notecraft_ui_egui::{LOGO_PNG, NoteCraftApp};

mod support;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Duration;
use support::clipboard::MemoryBoard;

/// Forwards the copies egui was asked to make in the frames just drawn to the test's clipboard,
/// as the system clipboard would receive them. egui's own copies leave a frame as output and
/// are never written to the real clipboard by a test.
fn forward_copies(h: &mut Harness<'_, NoteCraftApp>, board: &MemoryBoard) {
    for command in &h.output().platform_output.commands {
        if let egui::OutputCommand::CopyText(text) = command {
            board.put_text(text);
        }
    }
}

/// Presses Ctrl+`key` and lets the frames it takes run, forwarding what was copied.
fn press_copy(h: &mut Harness<'_, NoteCraftApp>, board: &MemoryBoard, key: &str) {
    ask(h, "ui.key", json!({"key": key, "ctrl": true})).expect("ctrl key");
    for _ in 0..8 {
        pump(h);
        forward_copies(h, board);
    }
}

const LIGHT: Theme = Theme { mode: Mode::Light, accent: Accent::Default };
const DARK: Theme = Theme { mode: Mode::Dark, accent: Accent::Default };

// The system clipboard is shared by parallel UI tests, even when each test has its own window.
fn params(pairs: &[(&str, &str)]) -> Params {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

fn session(t: &TempDir) -> Session {
    let settings = Settings::at(t.path().join("settings.toml"));
    settings.set("privacy_welcome_seen", "true").expect("test settings");
    // Most UI tests exercise the single-tree interactions explicitly. The production default
    // is the OneNote-style split layout; tests that cover that default opt in below.
    settings.set("split_panes", "false").expect("test navigation setting");
    // These tests use synthetic mouse input; keep the hardware-specific pressure warning out
    // of unrelated drawing interaction tests. The warning itself is exercised through the app
    // alert path and real hardware is covered by the pressure canvas.
    settings.set("pressure_unavailable_never", "true").expect("test pressure setting");
    Session::new(&t.path().join("notebooks"), settings).expect("session")
}

fn sample_app(label: &str) -> (TempDir, NoteCraftApp, Sample) {
    let t = TempDir::new(label);
    let mut s = session(&t);
    let sample = create_sample(&mut s, Some(LOGO_PNG)).expect("sample");
    let mut app = NoteCraftApp::new(s);
    app.set_redact_paths(true);
    (t, app, sample)
}

fn split_sample_app(label: &str) -> (TempDir, NoteCraftApp, Sample) {
    let t = TempDir::new(label);
    let mut s = session(&t);
    s.settings.set("split_panes", "true").expect("split test setting");
    let sample = create_sample(&mut s, Some(LOGO_PNG)).expect("sample");
    let mut app = NoteCraftApp::new(s);
    app.set_redact_paths(true);
    (t, app, sample)
}

fn pressure_session(t: &TempDir) -> Session {
    let settings = Settings::at(t.path().join("pressure-settings.toml"));
    settings.set("privacy_welcome_seen", "true").expect("test settings");
    settings.set("split_panes", "false").expect("test navigation setting");
    Session::new(&t.path().join("pressure-notebooks"), settings).expect("pressure session")
}

fn touch_event(phase: egui::TouchPhase, force: Option<f32>) -> egui::Event {
    egui::Event::Touch { device_id: egui::TouchDeviceId(0), id: egui::TouchId(0), phase, pos: egui::Pos2::new(100.0, 100.0), force }
}

fn harness(app: NoteCraftApp, theme: Theme) -> Harness<'static, NoteCraftApp> {
    sized_harness(app, theme, egui::vec2(1200.0, 760.0))
}

/// A window of a given size, for a test that needs more room than the usual one: the Draw
/// tab has more groups than fit in 1200 points, and a button that is scrolled off cannot
/// be clicked.
fn sized_harness(mut app: NoteCraftApp, theme: Theme, size: egui::Vec2) -> Harness<'static, NoteCraftApp> {
    app.set_theme(theme);
    Harness::builder().with_size(size).wgpu().build_ui_state(|ui, app: &mut NoteCraftApp| app.show(ui), app)
}

/// Feeds the clicks, keys and text that control requests queued into the next frame, then runs it.
fn pump(h: &mut Harness<'_, NoteCraftApp>) {
    let events = h.state_mut().take_events();
    h.input_mut().events.extend(events);
    h.step();
}

/// Steps until pictures loaded in the background have arrived.
///
/// It steps frames rather than calling `Harness::run`, which gives up when a window asks
/// to be repainted more than a few times. A page with unsaved changes asks for exactly
/// that until its autosave fires, so any test that had typed, drawn or inserted anything
/// tripped over `run` - four of them did, on four separate days, each looking like a
/// flake because it passed when run alone. Not calling `run` here is what stops the fifth.
fn settle(h: &mut Harness<'_, NoteCraftApp>) {
    for _ in 0..25 {
        pump(h);
        std::thread::sleep(Duration::from_millis(20));
    }
    pump(h);
}

/// Writes a review picture when `NOTECRAFT_UI_VISUAL_DIR` is set. Never asserts.
fn review(h: &mut Harness<'_, NoteCraftApp>, name: &str) {
    if let Some(dir) = std::env::var_os("NOTECRAFT_UI_VISUAL_DIR") {
        let dir = PathBuf::from(dir);
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(img) = h.render() {
            let _ = img.save(dir.join(format!("{name}.png")));
        }
    }
}

fn ask(h: &mut Harness<'_, NoteCraftApp>, method: &str, p: Value) -> Result<Value, String> {
    let (req, rx) = ControlRequest::new(method, p);
    let ctx = h.ctx.clone();
    h.state_mut().handle_control(&ctx, req);
    pump(h);
    rx.try_recv().unwrap_or_else(|_| Err("no answer".into()))
}

// ---------- behaviour ----------

#[test]
fn the_window_opens_and_shows_the_sample() {
    let (_t, app, _s) = sample_app("ui-open");
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    for label in ["My Notebook", "Archive", "New notebook", "Expand all", "Collapse all"] {
        let _ = h.get_by_label(label);
    }
    review(&mut h, "welcome_with_sample");
}

#[test]
fn mouse_warning_and_synthetic_ink_mode_cover_the_pressure_ribbon() {
    let t = TempDir::new("ui-pressure-modes");
    let mut s = pressure_session(&t);
    let sample = create_sample(&mut s, Some(LOGO_PNG)).expect("sample");
    let mut app = NoteCraftApp::new(s);
    app.set_redact_paths(true);
    app.open_path(&sample.md_page.to_string_lossy());
    let mut h = harness(app, LIGHT);
    settle(&mut h);

    // Mouse mode: pointer events arrive without force, so the warning is shown and the
    // sensitivity control is explicitly unavailable.
    h.get_by_label("Draw").click();
    h.run();
    assert!(h.query_all_by_label("Pressure sensitivity unavailable").count() > 0, "mouse mode warns");
    assert!(h.query_all_by_label("Don't show this again in this session").count() > 0, "session suppression is offered");
    assert!(h.query_all_by_label("Don't show this ever").count() > 0, "permanent suppression is offered");
    h.get_by_label("OK").click();
    h.run();
    assert!(h.query_all_by_label("Pressure off").count() > 0, "mouse mode disables sensitivity");

    // Ink mode: a force-bearing touch event is what the Windows input adapter supplies.
    h.input_mut().events.push(touch_event(egui::TouchPhase::Start, Some(0.25)));
    h.step();
    assert!(h.query_all_by_label("Pressure sensitivity 50 of 100").count() > 0, "ink mode enables sensitivity");
    h.input_mut().events.push(touch_event(egui::TouchPhase::Move, Some(0.8)));
    h.step();
    let diagnostics = ask(&mut h, "ui.inspect", json!({})).expect("inspect pressure")["pen"].clone();
    assert_eq!(diagnostics["seen"].as_bool(), Some(true), "the synthetic pen is detected: {diagnostics}");
    let pressure = diagnostics["pressure"].as_f64().unwrap_or_default();
    assert!((pressure - 0.8).abs() < 0.001, "the force reading reaches the app: {diagnostics}");
}

#[test]
fn clicking_a_page_opens_it_and_a_container_toggles() {
    let (_t, mut app, _s) = sample_app("ui-click");
    app.expand_all();
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    h.get_by_label("Project X").click();
    settle(&mut h);
    assert!(h.state().body().is_some_and(|b| b.contains("Kickoff notes")), "the HTML page is open");
    // This page holds a diagram, which is drawn on a thread of its own and keeps the window
    // asking to be repainted until it is done, so the frames are stepped and not `run`.
    h.get_by_label("Format ideas").click();
    settle(&mut h);
    assert!(h.state().body().is_some_and(|b| b.starts_with("# Format ideas")), "the Markdown page is open");
    review(&mut h, "markdown_page");
}

/// The caret lives in the page: clicking a block and typing changes that block, and the page
/// is written back in its own format. This is the editor the owner asked for, so it is tested
/// for both formats.
#[test]
fn writing_on_the_page_changes_the_block_and_the_file() {
    for (label, open_md) in [("ui-write-html", false), ("ui-write-md", true)] {
        let (t, mut app, s) = sample_app(label);
        app.open_path(&if open_md { s.md_page.to_string_lossy() } else { s.html_page.to_string_lossy() });
        let mut h = harness(app, LIGHT);
        settle(&mut h);
        let page = ask(&mut h, "ui.inspect", json!({})).expect("inspect")["page"].clone();
        assert_eq!(page["writable"], true, "{label}: a page NoteCraft wrote can be written on");
        assert_eq!(page["caret"], Value::Null, "{label}: no caret until the page is clicked in");

        // The caret goes in the first paragraph, and what is typed lands in that block.
        let before = h.state().body().unwrap_or_default().to_string();
        let put = ask(&mut h, "ui.caret", json!({"block": 1})).expect("caret");
        assert!(put["text"].as_str().is_some_and(|t| !t.is_empty()), "{label}: the block gives up its text");
        ask(&mut h, "ui.text", json!({"text": "NEW "})).expect("type");
        for _ in 0..4 {
            pump(&mut h);
        }
        let after = h.state().body().unwrap_or_default().to_string();
        assert_ne!(after, before, "{label}: the page changed");
        assert!(after.contains("NEW "), "{label}: what was typed is in the page:\n{after}");
        // The page is still in its own format, not the other one.
        if open_md {
            assert!(after.contains("## ") && !after.contains("<h2>"), "{label}: still Markdown:\n{after}");
        } else {
            assert!(after.contains("<h2>") && after.contains("<p>"), "{label}: still HTML:\n{after}");
        }

        // Enter starts a new block, and the file on disk carries the change.
        let blocks_before = ask(&mut h, "ui.inspect", json!({})).expect("inspect")["page"]["blocks"].as_u64().unwrap_or(0);
        ask(&mut h, "ui.key", json!({"key": "Enter"})).expect("enter");
        for _ in 0..4 {
            pump(&mut h);
        }
        let inspect = ask(&mut h, "ui.inspect", json!({})).expect("inspect");
        assert_eq!(inspect["page"]["blocks"].as_u64(), Some(blocks_before + 1), "{label}: Enter adds a block");
        assert_eq!(inspect["page"]["caret"]["block"].as_u64(), Some(2), "{label}: the caret moved to the new block");
        h.state_mut().save_page_now();
        let file = if open_md { s.md_page.join("index.md") } else { s.html_page.join("index.html") };
        let raw = std::fs::read_to_string(&file).expect("the page file");
        assert!(raw.contains("NEW "), "{label}: the file holds what was typed:\n{raw}");
        drop(t);
    }
}

/// A picture, a table or a rule cannot take the caret yet, and a page with markup the block
/// model cannot hold stays read-only instead of losing it.
#[test]
fn what_cannot_be_written_in_says_so() {
    let (_t, mut app, s) = sample_app("ui-readonly");
    app.open_path(&s.html_page.to_string_lossy());
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    // Every block of this page takes the caret, the table among them: its cells used to
    // refuse it, which made the page a viewer rather than an editor.
    let rows = ask(&mut h, "ui.inspect", json!({})).expect("inspect")["page"]["blocks"].as_u64().unwrap_or(0);
    assert!(rows > 0);
    for n in 0..rows {
        assert!(ask(&mut h, "ui.caret", json!({"block": n})).is_ok(), "block {n} takes the caret");
    }
    assert!(ask(&mut h, "ui.caret", json!({"block": 999})).is_err(), "a block that is not there is an error");

    // A page with markup the editor would drop is read-only, and says so rather than losing it.
    let dir = s.html_page.clone();
    h.state_mut()
        .session_mut()
        .run("page.save", &params(&[("path", &dir.to_string_lossy()), ("body", "<p>Keep me</p><aside data-x=\"1\">side</aside>")]))
        .expect("save");
    h.state_mut().open_path(&dir.to_string_lossy());
    settle(&mut h);
    let page = ask(&mut h, "ui.inspect", json!({})).expect("inspect")["page"].clone();
    assert_eq!(page["writable"], false, "unknown markup keeps the page read-only");
    assert!(ask(&mut h, "ui.caret", json!({"block": 0})).is_err());
    review(&mut h, "read_only_page");
}

/// Tags: the ribbon puts one on the line the caret is in, the page carries it, and asking
/// for the same tag again takes it off.
#[test]
fn tags_go_on_a_line_and_come_off_again() {
    let (_t, mut app, s) = sample_app("ui-tags");
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    ask(&mut h, "ui.caret", json!({"block": 1})).expect("caret");
    pump(&mut h);
    h.get_by_label("Important (Ctrl+2)").click();
    h.run();
    h.run();
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(body.lines().any(|l| l.starts_with("#important NoteCraft pages")), "the tag is in the page:\n{body}");
    // The same tag again removes it.
    h.get_by_label("Important (Ctrl+2)").click();
    h.run();
    h.run();
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(!body.contains("#important"), "the tag came off:\n{body}");
    // To Do is a task item, not a marker word.
    h.get_by_label("To Do Tag").click();
    h.run();
    h.run();
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(body.lines().any(|l| l.starts_with("- [ ] NoteCraft pages")), "a to-do line:\n{body}");
    review(&mut h, "tags");
}

/// Arabic draws, and the three direction controls do what they say: the page, the line the
/// caret is in, and the run inside it.
#[test]
fn arabic_draws_and_the_direction_controls_work() {
    let (_t, mut app, s) = sample_app("ui-rtl");
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = harness(app, LIGHT);
    settle(&mut h);

    // The bundled face has Arabic; egui's own faces do not, which is why it is bundled.
    assert!(h.ctx.fonts_mut(|f| f.has_glyphs(&egui::FontId::proportional(14.0), "مرحبا")), "Arabic would draw as empty boxes");

    // Type Arabic into a line: it is written to the page as typed.
    ask(&mut h, "ui.caret", json!({"block": 1})).expect("caret");
    pump(&mut h);
    ask(&mut h, "ui.text", json!({"text": "مرحبا "})).expect("type");
    pump(&mut h);
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(body.contains("مرحبا"), "what was typed is in the page:\n{body}");

    // The paragraph's direction sits where OneNote keeps it: in the alignment menu.
    h.get_by_label("Alignment and direction").click();
    h.run_steps(2);
    // A picture of the open menu: the two direction entries carry the pilcrow and arrow
    // that Word, LibreOffice and the Office web apps all use for paragraph direction.
    review(&mut h, "alignment-menu");
    h.get_by_label("Right-to-Left").click();
    h.run_steps(2);
    let body = h.state().body().unwrap_or_default().to_string();
    let marked: Vec<&str> = body.lines().filter(|l| l.starts_with('\u{200F}')).collect();
    assert_eq!(marked.len(), 1, "one paragraph carries the mark:\n{body:?}");
    h.get_by_label("Alignment and direction").click();
    h.run_steps(2);
    h.get_by_label("Direction from the words").click();
    h.run_steps(2);
    assert!(!h.state().body().unwrap_or_default().contains('\u{200F}'), "auto takes the mark off");

    // The paragraph's direction is also in the Direction group, with the pilcrow-and-arrow
    // mark Word uses for it, so a writer looking for the direction controls finds them all
    // in one place instead of under an alignment button.
    h.get_by_label("Paragraph right to left").click();
    h.run_steps(2);
    assert_eq!(
        h.state().body().unwrap_or_default().lines().filter(|l| l.starts_with('\u{200F}')).count(),
        1,
        "the group's button marks the paragraph the menu's entry does"
    );
    h.get_by_label("Paragraph direction from the words").click();
    h.run_steps(2);
    assert!(!h.state().body().unwrap_or_default().contains('\u{200F}'), "and takes it off again");

    // The page's own direction is written into the page's file, and the window redraws by
    // it: before this was fixed the file changed and nothing on the screen moved.
    h.get_by_label("Page right to left").click();
    h.run_steps(2);
    let file = s.md_page.join("index.md");
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    assert!(text.contains("dir: rtl"), "the page records it:\n{text}");
    h.get_by_label("Page direction from the words").click();
    h.run_steps(2);
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    assert!(!text.contains("dir: rtl"), "and auto takes it out again:\n{text}");

    // The less common run direction sits in a compact More menu, like OneNote's paragraph
    // direction entries, rather than opening a separate dialog.
    h.get_by_label("Text run direction").click();
    h.run_steps(2);
    review(&mut h, "run-direction-menu");
    h.get_by_label("Right-to-Left").click();
    h.run_steps(2);
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(body.contains('\u{2067}') && body.contains('\u{2069}'), "an isolate went in (status: {}):\n{body:?}", h.state().status());

    // The page control writes into the page's own file, where it travels with the folder.
    h.get_by_label("Page right to left").click();
    h.run_steps(2);
    let file = std::fs::read_to_string(s.md_page.join("index.md")).unwrap_or_default();
    assert!(file.contains("dir: rtl"), "the page's own direction is in its front matter:\n{file}");
    review(&mut h, "rtl");
}

/// The Justify button on the Home tab fills the column with the paragraph the caret is in,
/// and the page remembers which paragraph that is without a tatweel reaching the file.
///
/// This is 1.8 of the owner's right-to-left bar, end to end: the button, the page file and
/// the painter. What the stretching itself does is measured in `tests/rtl_quality.rs`.
#[test]
fn the_justify_button_fills_the_column_and_the_page_keeps_the_words() {
    // Long enough to wrap to several lines in the page's column, so there is something to
    // fill: a paragraph that fits on one line is all last line, and the last line is never
    // justified.
    const PARAGRAPH: &str = "الملاحظات يجب أن تعيش أطول من البرنامج الذي كتبها، ولهذا تحفظ كل صفحة في مجلد من ملفات نصية عادية يقرؤها أي محرر. الدفتر مجلد، والقسم مجلد، والصفحة مجلد فيه ملف واحد وصوره ومرفقاته، ولا شيء غير ذلك. لا قاعدة بيانات، ولا صيغة لا يفتحها إلا برنامج واحد.";
    let (_t, app, s) = sample_app("ui-justify");
    let mut h = harness(app, LIGHT);
    h.run();
    let ideas = s.md_page.parent().expect("section").to_string_lossy().into_owned();
    let made = ask(
        &mut h,
        "engine.execute",
        json!({"command": "node.create", "params": {"parent": ideas, "kind": "page", "title": "من اليمين إلى اليسار", "format": "cmgfm"}}),
    )
    .expect("page");
    let page = made.as_str().expect("a path").to_string();
    ask(&mut h, "engine.execute", json!({"command": "page.save", "params": {"path": page, "body": PARAGRAPH}})).expect("save");
    ask(&mut h, "ui.open", json!({"path": page})).expect("open");
    settle(&mut h);

    ask(&mut h, "ui.caret", json!({"block": 0})).expect("caret");
    pump(&mut h);
    h.get_by_label("Alignment and direction").click();
    h.run_steps(2);
    h.get_by_label("Justify High").click();
    h.run_steps(2);

    // The page knows which paragraph and how hard it is stretched, and its words are the
    // ones that were typed. The three strengths are Word's Justify Low, Medium and High.
    let file = PathBuf::from(&page).join("index.md");
    let text = std::fs::read_to_string(&file).expect("the page file");
    assert!(text.contains("justify: 0:high"), "the page records the paragraph and its strength:\n{text}");
    assert!(!text.contains('\u{0640}'), "no tatweel is written into the file; the stretching is drawing only");
    assert!(text.contains(PARAGRAPH), "the words are the ones that were typed");
    assert_eq!(h.state().body().unwrap_or_default().trim(), PARAGRAPH, "and the window agrees with the folder");

    // The caret leaves the paragraph, so the picture shows the paragraph as it is read
    // rather than as it is written in: while the caret is in a block it is a text box of
    // the plain words, and the stretching belongs to the painter.
    ask(&mut h, "ui.key", json!({"key": "Escape"})).expect("leave the block");
    settle(&mut h);
    // The page is named in Arabic, so the same picture shows the tree row and the title
    // band drawing that name in reading order; `tests/rtl_quality.rs` measures it.
    review(&mut h, "justify");

    // A different strength changes the paragraph rather than adding a second entry.
    ask(&mut h, "ui.caret", json!({"block": 0})).expect("caret");
    pump(&mut h);
    h.get_by_label("Alignment and direction").click();
    h.run_steps(2);
    h.get_by_label("Justify Low").click();
    h.run_steps(2);
    let text = std::fs::read_to_string(&file).expect("the page file");
    assert!(text.contains("justify: 0:low"), "another strength changes it rather than adding a second entry:\n{text}");

    // Asking for the strength it already has puts the paragraph back the way it was, which
    // is how a menu of three gives a way to a ragged edge without a fourth entry.
    h.get_by_label("Alignment and direction").click();
    h.run_steps(2);
    h.get_by_label("Justify Low").click();
    h.run_steps(2);
    let text = std::fs::read_to_string(&file).expect("the page file");
    assert!(!text.contains("justify:"), "the line is gone, not left empty:\n{text}");
}

/// A table copied out of another note taking app arrives as tab-separated text. It must come
/// back as a table, not as one long line.
#[test]
fn a_table_pasted_from_another_app_lands_as_a_table() {
    let (_t, mut app, s) = sample_app("ui-paste-table");
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    ask(&mut h, "ui.caret", json!({"block": 1})).expect("caret");
    pump(&mut h);
    // Exactly what the clipboard holds after copying a small table in OneNote.
    ask(&mut h, "ui.paste", json!({"text": "test\t\t\t\n\t\t\t\n\ttest\t\t\n"})).expect("paste");
    pump(&mut h);
    pump(&mut h);
    let body = h.state().body().unwrap_or_default().to_string();
    let pipes = body.lines().filter(|l| l.starts_with('|')).count();
    assert!(pipes >= 3, "the page has a table in it:\n{body}");
    assert!(body.contains("test"), "with the cells that were copied:\n{body}");
}

/// Writing in an Arabic line: the caret is NoteCraft's own there, so a click lands on the
/// character under the pointer and the arrow keys step the way the eye moves.
#[test]
fn the_caret_in_an_arabic_line_is_ours_and_behaves() {
    let (_t, mut app, s) = sample_app("ui-rtl-caret");
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    // A line of Arabic with a number in it: both the things egui gets wrong.
    ask(&mut h, "ui.caret", json!({"block": 1})).expect("caret");
    pump(&mut h);
    ask(&mut h, "ui.text", json!({"text": "السعر 17 ريال"})).expect("type");
    pump(&mut h);
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(body.contains("السعر 17 ريال"), "typed into the page:\n{body}");

    // Typing more goes in at the caret, not at the far end: the text keeps its order.
    ask(&mut h, "ui.text", json!({"text": "."})).expect("type");
    pump(&mut h);
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(body.contains("السعر 17 ريال."), "the full stop went in after what was typed:\n{body}");

    // The arrow keys move without the text changing, and without a panic.
    for key in ["ArrowLeft", "ArrowRight", "Home", "End"] {
        ask(&mut h, "ui.key", json!({"key": key})).expect("key");
        pump(&mut h);
    }
    let after = h.state().body().unwrap_or_default().to_string();
    assert_eq!(after, body, "moving the caret does not change the text");

    // Backspace takes off one character of the Arabic, not a glyph and not the line.
    ask(&mut h, "ui.key", json!({"key": "End"})).expect("end");
    pump(&mut h);
    ask(&mut h, "ui.key", json!({"key": "Backspace"})).expect("backspace");
    pump(&mut h);
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(body.contains("السعر 17 ريال") && !body.contains("السعر 17 ريال."), "one character came off:\n{body}");
}

/// Selecting inside an Arabic line: dragging, Shift with the arrows, and a double click.
#[test]
fn a_selection_can_be_made_inside_an_arabic_line() {
    let (_t, mut app, s) = sample_app("ui-rtl-select");
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    ask(&mut h, "ui.caret", json!({"block": 1})).expect("caret");
    pump(&mut h);
    ask(&mut h, "ui.text", json!({"text": "السعر 123 ريال"})).expect("type");
    pump(&mut h);
    let selection = |h: &mut Harness<'_, NoteCraftApp>| -> (u64, u64) {
        let v = ask(h, "ui.inspect", json!({})).expect("inspect");
        let caret = v.pointer("/page/caret").cloned().unwrap_or(Value::Null);
        (caret.get("at").and_then(Value::as_u64).unwrap_or(0), caret.get("anchor").and_then(Value::as_u64).unwrap_or(0))
    };
    let (at, anchor) = selection(&mut h);
    assert_eq!(at, anchor, "nothing is selected after typing");

    // Shift with an arrow extends the selection from where the caret is.
    ask(&mut h, "ui.key", json!({"key": "ArrowLeft", "shift": true})).expect("shift left");
    pump(&mut h);
    let (at2, anchor2) = selection(&mut h);
    assert_eq!(anchor2, anchor, "the selection still begins where the caret was");
    assert_ne!(at2, anchor2, "and now something is selected");

    // An arrow without Shift drops it again.
    ask(&mut h, "ui.key", json!({"key": "ArrowRight"})).expect("right");
    pump(&mut h);
    let (at3, anchor3) = selection(&mut h);
    assert_eq!(at3, anchor3, "moving without Shift clears the selection");
    assert_ne!(at3, 0, "and the caret is still in the line");
}

/// Clicking into a line must not change what the line says. The tag's marker is drawn as a
/// mark beside it, and it stays a mark while the line is written in.
#[test]
fn a_line_looks_the_same_when_the_caret_arrives() {
    let (_t, mut app, s) = sample_app("ui-no-jump");
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    ask(&mut h, "ui.caret", json!({"block": 1})).expect("caret");
    pump(&mut h);
    h.get_by_label("Important (Ctrl+2)").click();
    h.run();
    h.run();
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(body.contains("#important "), "the marker is in the file:\n{body}");

    // The caret is in that line, and what it holds is the words, not the marker.
    let caret = ask(&mut h, "ui.inspect", json!({})).expect("inspect").pointer("/page/caret").cloned().unwrap_or(Value::Null);
    let text = caret.get("text").and_then(Value::as_str).unwrap_or_default().to_string();
    assert!(!text.contains("#important"), "the line being written in shows no marker: {text:?}");
    assert!(text.starts_with("NoteCraft pages"), "it shows the words: {text:?}");

    // Leaving and coming back keeps both: the words on screen, the marker in the file.
    ask(&mut h, "ui.caret", json!({"block": 2})).expect("caret");
    pump(&mut h);
    ask(&mut h, "ui.caret", json!({"block": 1})).expect("caret");
    pump(&mut h);
    let again =
        ask(&mut h, "ui.inspect", json!({})).expect("inspect").pointer("/page/caret/text").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    assert_eq!(again, text, "the line reads the same the second time");
    assert!(h.state().body().unwrap_or_default().contains("#important "), "and the file still has the tag");
}

/// A page arranged in containers opens that way, and the arrangement is the page's own.
#[test]
fn a_page_opens_in_the_boxes_it_was_arranged_in() {
    let (_t, mut app, s) = sample_app("ui-boxes");
    let page = s.md_page.to_string_lossy().to_string();
    app.open_path(&page);
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    let blocks = ask(&mut h, "ui.inspect", json!({})).expect("inspect").pointer("/page/blocks").and_then(Value::as_u64).unwrap_or(0);
    assert!(blocks >= 3, "the sample page has blocks in it: {blocks}");

    // One box for everything is what a page that was never arranged gets.
    let boxes = |h: &mut Harness<'_, NoteCraftApp>| -> Vec<Value> {
        ask(h, "ui.inspect", json!({})).expect("inspect").pointer("/page/containers").and_then(Value::as_array).cloned().unwrap_or_default()
    };
    assert_eq!(boxes(&mut h).len(), 1, "one box holding the page");

    // Arrange it in two, the way dragging one would, and open it again.
    let line = format!("0,0,320,2; 360,30,300,{}", blocks - 2);
    ask(&mut h, "engine.execute", json!({"command": "page.containers", "params": {"path": page.clone(), "boxes": line}})).expect("arrange");
    ask(&mut h, "ui.open", json!({"path": page})).expect("open");
    pump(&mut h);
    let now = boxes(&mut h);
    assert_eq!(now.len(), 2, "two boxes: {now:?}");
    assert_eq!(now.first().and_then(|b| b.get("width")).and_then(Value::as_f64), Some(320.0));
    assert_eq!(now.get(1).and_then(|b| b.get("y")).and_then(Value::as_f64), Some(30.0));
    let counts: u64 = now.iter().filter_map(|b| b.get("blocks").and_then(Value::as_u64)).sum();
    assert_eq!(counts, blocks, "every block is in a box");
}

/// A container is moved by the bar along its top, and the page keeps where it was put.
#[test]
fn a_container_can_be_dragged_and_stays_where_it_is_put() {
    let (_t, mut app, s) = sample_app("ui-drag-box");
    let page = s.md_page.to_string_lossy().to_string();
    app.open_path(&page);
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    let look = |h: &mut Harness<'_, NoteCraftApp>| -> Value { ask(h, "ui.inspect", json!({})).expect("inspect") };
    let blocks = look(&mut h).pointer("/page/blocks").and_then(Value::as_u64).unwrap_or(0);
    let line = format!("40,20,320,{blocks}");
    ask(&mut h, "engine.execute", json!({"command": "page.containers", "params": {"path": page.clone(), "boxes": line}})).expect("arrange");
    ask(&mut h, "ui.open", json!({"path": page.clone()})).expect("open");
    settle(&mut h);

    // Where the box's handle is drawn: the page's own origin plus the box's place.
    let v = look(&mut h);
    let origin = v.pointer("/page/origin").and_then(Value::as_array).cloned().unwrap_or_default();
    let (ox, oy) = (origin.first().and_then(Value::as_f64).unwrap_or(0.0), origin.get(1).and_then(Value::as_f64).unwrap_or(0.0));
    assert!(ox > 0.0 && oy > 0.0, "the page knows where it is drawn: {origin:?}");
    let (hx, hy) = (ox + 40.0 + 160.0, oy + 20.0 + 6.0);

    // Press on the bar, move, let go.
    ask(&mut h, "ui.press", json!({"x": hx, "y": hy})).expect("press");
    pump(&mut h);
    ask(&mut h, "ui.move", json!({"x": hx + 60.0, "y": hy + 35.0})).expect("move");
    pump(&mut h);
    pump(&mut h);
    ask(&mut h, "ui.release", json!({"x": hx + 60.0, "y": hy + 35.0})).expect("release");
    pump(&mut h);
    settle(&mut h);

    // The box moved, and the page's own file says so.
    let now = look(&mut h).pointer("/page/containers/0").cloned().unwrap_or(Value::Null);
    let x = now.get("x").and_then(Value::as_f64).unwrap_or(0.0);
    let y = now.get("y").and_then(Value::as_f64).unwrap_or(0.0);
    assert!(x > 60.0 && y > 30.0, "the box followed the pointer: {now:?}");
    let file = std::fs::read_to_string(s.md_page.join("index.md")).unwrap_or_default();
    // The box is an element in the page's own text, with its place on it, and the old header
    // line is not used.
    assert!(file.contains("class=\"nc-box\"") && file.contains("data-x="), "it is written in the page's text:\n{file}");
    assert!(!file.contains("containers:"), "and not on a line in the front matter:\n{file}");
    // What the source pane shows is the same text, so the window and the file say one thing.
    let shown = h.state().body().unwrap_or_default().to_string();
    assert!(shown.contains("class=\"nc-box\""), "the source pane shows the box:\n{shown}");
}

/// The side panel can be split the way OneNote splits it: notebooks and sections in one
/// column, the pages of the chosen section in the next, and the three-bar button hides the
/// first of them.
#[test]
fn the_notebooks_and_the_pages_can_have_columns_of_their_own() {
    let (_t, mut app, s) = split_sample_app("ui-panes");
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    let panes = |h: &mut Harness<'_, NoteCraftApp>| -> (bool, bool) {
        let v = ask(h, "ui.inspect", json!({})).expect("inspect");
        (
            v.pointer("/panes/split").and_then(Value::as_bool).unwrap_or(false),
            v.pointer("/panes/organisers").and_then(Value::as_bool).unwrap_or(false),
        )
    };
    assert_eq!(panes(&mut h), (true, true), "OneNote's two columns to begin with");

    // The three-bar button hides the notebooks column.
    h.get_by_label("\u{2630}").click();
    h.run();
    h.run();
    assert_eq!(panes(&mut h), (true, false), "split, notebooks hidden");
    // Pressing it again brings the notebooks back.
    h.get_by_label("\u{2630}").click();
    h.run();
    h.run();
    assert_eq!(panes(&mut h), (true, true), "all three columns");

    // The page still works: the rows listed are the ones in the open section.
    let rows = ask(&mut h, "ui.inspect", json!({})).expect("inspect").pointer("/rows").and_then(Value::as_array).cloned().unwrap_or_default();
    assert!(!rows.is_empty(), "the tree still reports its rows");
    review(&mut h, "panes");
}

/// The line between two pages, which OneNote shows on hover, makes a page in that place.
#[test]
fn a_page_can_be_added_between_two_others() {
    let (_t, mut app, s) = sample_app("ui-insert-page");
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    // The two columns, so the pages have a column of their own with the lines in it.
    h.get_by_label("\u{2630}").click();
    h.run();
    h.get_by_label("\u{2630}").click();
    settle(&mut h);

    let titles = |h: &mut Harness<'_, NoteCraftApp>| -> Vec<String> {
        ask(h, "ui.inspect", json!({}))
            .expect("inspect")
            .pointer("/rows")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|r| r.get("title").and_then(Value::as_str).map(str::to_string))
            .collect()
    };
    let before = titles(&mut h);

    // Every gap offers a page; the first one puts it above everything else.
    let gaps = h.get_all_by_label("Add a page here").count();
    assert!(gaps >= 2, "a line above the first page and below the last: {gaps}");
    h.get_all_by_label("Add a page here").next().expect("a gap").click();
    h.run();
    settle(&mut h);

    let after = titles(&mut h);
    assert_eq!(after.len(), before.len() + 1, "one page more:\n{before:?}\n{after:?}");
    // It went in where the line was, not at the end of the section.
    let pages: Vec<&String> = after.iter().filter(|t| *t == "Untitled page").collect();
    assert!(!pages.is_empty(), "the new page is there: {after:?}");
    assert_ne!(after.last(), Some(&"Untitled page".to_string()), "and not merely appended: {after:?}");

    let state = ask(&mut h, "ui.inspect", json!({})).expect("inspect the new page");
    assert_eq!(state.pointer("/page/titleArea").and_then(Value::as_str), Some(""), "the title area is blank");
    let path = state.pointer("/page/path").and_then(Value::as_str).expect("the open page path");
    let rows = state.pointer("/rows").and_then(Value::as_array).expect("tree rows");
    assert!(
        rows.iter().any(|row| {
            row.get("path").and_then(Value::as_str) == Some(path) && row.get("title").and_then(Value::as_str) == Some("Untitled page")
        }),
        "the tree shows the fallback for the newly added page: {rows:?}"
    );
}

/// The "+" between two pages is a circle. It is wider than the thin gap it sits in, so it
/// is only round if the painter is given room for it; clipped to the gap it comes out a
/// lozenge, flat at the top and the bottom. Measured in the pixels, since that is the only
/// place the difference shows.
#[test]
fn the_plus_between_two_pages_is_round() {
    let (_t, mut app, s) = sample_app("ui-gap-circle");
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    // Two columns, so the pages have a column of their own with the lines in it.
    h.get_by_label("\u{2630}").click();
    h.run();
    h.get_by_label("\u{2630}").click();
    settle(&mut h);
    let gap = h.get_all_by_label("Add a page here").next().expect("a gap").rect();
    // Hover it, which is what draws the line and the button.
    ask(&mut h, "ui.move", json!({"x": gap.center().x, "y": gap.center().y})).expect("hover");
    for _ in 0..4 {
        pump(&mut h);
    }
    let shot = h.render().expect("a picture");
    // The harness is 1200 points wide; the picture may be rendered larger than that.
    let scale = f64::from(shot.width()) / 1200.0;

    // The accent-coloured blob around the left end of the gap: how wide, and how tall.
    let accent = LIGHT.tokens().accent;
    let near = |p: &image::Rgba<u8>| {
        let d = |a: u8, b: u8| i32::from(a).abs_diff(i32::from(b));
        d(p.0[0], accent.r()) + d(p.0[1], accent.g()) + d(p.0[2], accent.b()) < 40
    };
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
    let look = egui::Rect::from_center_size(egui::pos2(gap.min.x + 9.0, gap.center().y), egui::vec2(30.0, 30.0));
    for y in (look.min.y.max(0.0) as u32)..(look.max.y as u32).min(shot.height()) {
        for x in (look.min.x.max(0.0) as u32)..(look.max.x as u32).min(shot.width()) {
            let px = (f64::from(x) * scale) as u32;
            let py = (f64::from(y) * scale) as u32;
            if px < shot.width() && py < shot.height() && near(shot.get_pixel(px, py)) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    assert!(x1 > x0 && y1 > y0, "the button was drawn: {x0},{y0} to {x1},{y1}");
    let (w, hgt) = (f64::from(x1 - x0), f64::from(y1 - y0));
    assert!((w - hgt).abs() <= 2.0, "the + is as tall as it is wide: {w} by {hgt}");
    review(&mut h, "page_gap_plus");
}

/// Opening a notebook from somewhere else must not put away the ones already open, and it
/// must still be open next time the window starts.
#[test]
fn a_notebook_opened_from_elsewhere_joins_the_others() {
    let (t, app, _s) = sample_app("ui-open-elsewhere");
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    let count = |h: &mut Harness<'_, NoteCraftApp>| h.state().session().tree().len();
    let before = count(&mut h);
    assert!(before >= 1, "the sample notebooks are open");

    // A notebook of its own, outside the notebooks folder.
    let away = t.path().join("away");
    std::fs::create_dir_all(&away).expect("dir");
    let made = {
        let settings = notecraft_store::Settings::at(t.path().join("settings").join("s.toml"));
        let mut other = notecraft_engine::Session::new(&away, settings).expect("session");
        match other.run("notebook.create", &notecraft_engine::Params::from([("name".to_string(), "Elsewhere".to_string())])) {
            Ok(notecraft_engine::Output::Path(p)) => p,
            other => panic!("expected a path, got {other:?}"),
        }
    };

    ask(&mut h, "engine.execute", json!({"command": "notebook.open", "params": {"path": made.to_string_lossy()}})).expect("open");
    pump(&mut h);
    assert_eq!(count(&mut h), before + 1, "it joined the others rather than replacing them");
    assert!(h.state().session().tree().iter().any(|n| n.info.title == "Elsewhere"));

    // Closing it leaves the rest alone.
    ask(&mut h, "engine.execute", json!({"command": "notebook.close", "params": {"path": made.to_string_lossy()}})).expect("close");
    pump(&mut h);
    assert_eq!(count(&mut h), before, "and goes away again without taking anything with it");
    assert!(made.is_dir(), "the folder is untouched");
}

/// The window comes back on the page it closed on.
#[test]
fn the_last_page_opens_again_next_time() {
    let (t, mut app, s) = sample_app("ui-restore");
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    assert!(h.state().body().is_some_and(|b| b.starts_with("# Format ideas")));
    drop(h);
    // A new window on the same notebooks folder and settings file.
    let again = NoteCraftApp::new(session(&t));
    let mut h2 = harness(again, LIGHT);
    settle(&mut h2);
    let inspect = ask(&mut h2, "ui.inspect", json!({})).expect("inspect");
    assert_eq!(inspect["page"]["path"].as_str(), Some("my-notebook/ideas/format-ideas"), "the same page is open");
    // Its section is open in the tree, so the page can be seen.
    let rows = inspect["rows"].as_array().cloned().unwrap_or_default();
    assert!(rows.iter().any(|r| r["path"] == "my-notebook/ideas" && r["open"] == true), "the branches above it are open");
}

#[test]
fn deep_pages_expand_and_collapse_at_every_level() {
    let (_t, app, _s) = sample_app("ui-deep");
    let mut h = harness(app, LIGHT);
    h.run();
    let rows = |h: &mut Harness<'_, NoteCraftApp>| ask(h, "ui.inspect", json!({})).expect("inspect")["rows"].as_array().map(Vec::len).unwrap_or(0);
    assert_eq!(rows(&mut h), 2, "collapsed: the two notebooks");
    h.get_by_label("Expand all").click();
    h.run();
    assert_eq!(rows(&mut h), 14, "every level open: all 14 nodes, the deepest ten levels down");
    review(&mut h, "tree_expanded");
    h.get_by_label("Collapse all").click();
    h.run();
    assert_eq!(rows(&mut h), 2);
    review(&mut h, "tree_collapsed");
}

/// The ribbon: its tabs change what is shown, and a button on the Insert tab puts a table in
/// the page, in whichever format that page is.
#[test]
fn the_ribbon_inserts_in_both_formats() {
    let (_t, mut app, s) = sample_app("ui-ribbon");
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = harness(app, LIGHT);
    h.run();
    // Home is the tab that opens, and its buttons are there.
    for label in ["Home", "Insert", "View", "Bold", "Heading 1", "Bullets", "To Do Tag"] {
        let _ = h.get_by_label(label);
    }
    // Undo names both the group and the button in it, so both are expected.
    assert!(h.query_all_by_label("Undo").count() >= 2, "the Undo group and its button");
    assert!(h.query_all_by_label("Redo").count() >= 1, "and Redo beside it");
    review(&mut h, "tab_home");
    h.get_by_label("Insert").click();
    h.run();
    // The Table button opens OneNote's size picker, and a square in it says how big.
    h.get_by_label("Table").click();
    h.run();
    review(&mut h, "table_picker");
    h.get_by_label("3 x 2 Table").click();
    h.run();
    let empty = "|  |  |  |
| --- | --- | --- |
|  |  |  |
";
    assert!(h.state().body().is_some_and(|b| b.contains(empty)), "three columns and two rows, all empty: {:?}", h.state().body());
    assert!(h.state().is_dirty());
    h.state_mut().save();
    assert!(!h.state().is_dirty());
    review(&mut h, "ribbon_insert");

    h.state_mut().open_path(&s.html_page.to_string_lossy());
    settle(&mut h);
    h.get_by_label("Table").click();
    h.run();
    h.get_by_label("3 x 2 Table").click();
    h.run();
    assert!(h.state().body().is_some_and(|b| b.contains("<table>") && b.contains("<th>")), "{:?}", h.state().body());
}

/// The ribbon's Styles and Basic Text buttons change the block the caret is in.
#[test]
fn the_ribbon_formats_what_the_caret_is_in() {
    let (_t, mut app, s) = sample_app("ui-ribbon-format");
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    ask(&mut h, "ui.caret", json!({"block": 1})).expect("caret");
    pump(&mut h);
    h.get_by_label("Heading 2").click();
    h.run();
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(body.lines().any(|l| l.starts_with("## NoteCraft pages")), "the paragraph became a heading:\n{body}");
    // And back to ordinary text with the same button.
    h.get_by_label("Heading 2").click();
    h.run();
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(body.lines().any(|l| l.starts_with("NoteCraft pages")), "and back again:\n{body}");
    // A to-do box on the same line.
    h.get_by_label("To Do Tag").click();
    h.run();
    h.run();
    let body = h.state().body().unwrap_or_default().to_string();
    let inspect = ask(&mut h, "ui.inspect", json!({})).expect("inspect");
    let (status, caret) = (inspect["status"]["text"].clone(), inspect["page"]["caret"].clone());
    assert!(body.lines().any(|l| l.starts_with("- [ ] NoteCraft pages")), "a to-do line (status {status}, caret {caret}):\n{body}");
}

#[test]
fn creating_a_notebook_through_the_dialog() {
    let t = TempDir::new("ui-newnb");
    let mut app = NoteCraftApp::new(session(&t));
    app.set_redact_paths(true);
    let mut h = harness(app, LIGHT);
    h.run();
    h.get_by_label("New notebook").click();
    h.run();
    h.input_mut().events.push(egui::Event::Text("Work".into()));
    h.run();
    review(&mut h, "new_notebook_dialog");
    h.get_by_label("Create").click();
    h.run();
    let titles: Vec<String> = h.state().session().tree().iter().map(|n| n.info.title.clone()).collect();
    assert_eq!(titles, vec!["Work".to_string()]);
}

#[test]
fn the_default_format_selector_changes_the_global_setting() {
    let t = TempDir::new("ui-format");
    let mut h = harness(NoteCraftApp::new(session(&t)), LIGHT);
    h.run();
    assert_eq!(h.state().session().settings.global_format().0.id(), "html");
    h.get_by_role(egui::accesskit::Role::ComboBox).click();
    h.run();
    h.get_by_label("CommonMark + GFM").click();
    h.run();
    assert_eq!(h.state().session().settings.global_format().0.id(), "cmgfm");
}

// ---------- the control channel drives the window ----------

#[test]
fn control_requests_run_commands_and_drive_input() {
    let (_t, app, _s) = sample_app("ui-control");
    let mut h = harness(app, LIGHT);
    h.run();
    // Commands.
    let commands = ask(&mut h, "engine.commands", json!({})).expect("commands");
    assert!(commands.as_array().is_some_and(|a| a.iter().any(|c| c["id"] == "node.create")));
    let made = ask(&mut h, "engine.execute", json!({"command": "notebook.create", "params": {"name": "From control"}})).expect("execute");
    assert!(made.as_str().is_some_and(|p| p.ends_with("from-control")));
    let inspect = ask(&mut h, "ui.inspect", json!({})).expect("inspect");
    assert_eq!(inspect["rows"].as_array().map(Vec::len), Some(3), "the window shows the new notebook at once");
    assert_eq!(inspect["theme"]["id"], "light-default");
    // Errors are answers, not panics.
    assert!(ask(&mut h, "engine.execute", json!({"command": "no.such"})).is_err());
    assert!(ask(&mut h, "ui.key", json!({"key": "NotAKey"})).is_err());
    assert!(ask(&mut h, "no.such.method", json!({})).is_err());
    // Real pointer and keyboard input: click "New notebook", type a name, press Enter.
    let rect = h.get_by_label("New notebook").rect();
    ask(&mut h, "ui.click", json!({"x": rect.center().x, "y": rect.center().y})).expect("click");
    for _ in 0..6 {
        pump(&mut h);
    }
    ask(&mut h, "ui.text", json!({"text": "Typed by control"})).expect("text");
    for _ in 0..3 {
        pump(&mut h);
    }
    ask(&mut h, "ui.key", json!({"key": "Enter"})).expect("key");
    for _ in 0..6 {
        pump(&mut h);
    }
    let titles: Vec<String> = h.state().session().tree().iter().map(|n| n.info.title.clone()).collect();
    assert!(titles.contains(&"Typed by control".to_string()), "{titles:?}");
    // Themes and opening pages.
    assert_eq!(ask(&mut h, "ui.theme", json!({"mode": "dark", "accent": "purple"})).expect("theme")["theme"], "dark-purple");
    assert!(ask(&mut h, "ui.theme", json!({"accent": "pink"})).is_err());
    let opened = ask(&mut h, "ui.open", json!({"path": "my-notebook/ideas/format-ideas"})).expect("open");
    assert_eq!(opened["page"]["format"], "cmgfm");
    assert!(ask(&mut h, "ui.open", json!({"path": "../outside"})).is_err());
}

/// A link in a page is followed with a plain click when the caret is not in its line: a
/// relative link opens the page it names, a broken one says so and stays put, a web address
/// is handed to the browser. The resolver itself is tested in `notecraft-store`.
#[test]
fn clicking_a_link_opens_the_page_it_names() {
    let (_t, app, s) = sample_app("ui-link");
    let mut h = harness(app, LIGHT);
    h.run();
    let ideas = s.md_page.parent().expect("section").to_string_lossy().into_owned();
    let made = ask(
        &mut h,
        "engine.execute",
        json!({"command": "node.create", "params": {"parent": ideas, "kind": "page", "title": "Links", "format": "cmgfm"}}),
    )
    .expect("page");
    let page = made.as_str().expect("a path").to_string();
    let body = "[Open the format page](../format-ideas/)\n\n[A page that is not there](../nowhere/)\n\n[The web](https://example.test/)\n";
    ask(&mut h, "engine.execute", json!({"command": "page.save", "params": {"path": page, "body": body}})).expect("save");
    let click = |h: &mut Harness<'static, NoteCraftApp>, label: &str| {
        let rect = h.get_by_label(label).rect();
        ask(h, "ui.click", json!({"x": rect.center().x, "y": rect.center().y})).expect("click");
        for _ in 0..6 {
            pump(h);
        }
    };

    ask(&mut h, "ui.open", json!({"path": page})).expect("open");
    settle(&mut h);
    click(&mut h, "Open the format page");
    assert!(h.state().body().is_some_and(|b| b.starts_with("# Format ideas")), "the linked page is open, not {:?}", h.state().body());

    ask(&mut h, "ui.open", json!({"path": page})).expect("open again");
    settle(&mut h);
    click(&mut h, "A page that is not there");
    assert!(h.state().status().contains("Nothing at"), "a broken link is reported: {}", h.state().status());
    assert!(h.state().body().is_some_and(|b| b.starts_with("[Open the format page]")), "and the page stays open");

    click(&mut h, "The web");
    assert_eq!(h.state().status(), "Opened https://example.test/ in the browser");
    review(&mut h, "links_page");
}

/// A link in a right-to-left line. Those lines are drawn by the bidi painter, which paints
/// straight onto the canvas, so two things have to hold: the line says what it says to a
/// screen reader and to this test, and a click on it is mapped through the painter that drew
/// it rather than through egui's own layout, which puts such a line in the wrong order.
#[test]
fn clicking_a_link_in_a_right_to_left_line_opens_the_page_it_names() {
    let (_t, app, s) = sample_app("ui-rtl-link");
    let mut h = harness(app, LIGHT);
    h.run();
    let ideas = s.md_page.parent().expect("section").to_string_lossy().into_owned();
    let made = ask(
        &mut h,
        "engine.execute",
        json!({"command": "node.create", "params": {"parent": ideas, "kind": "page", "title": "Arabic links", "format": "cmgfm"}}),
    )
    .expect("page");
    let page = made.as_str().expect("a path").to_string();
    ask(&mut h, "engine.execute", json!({"command": "page.save", "params": {"path": page, "body": "[صفحة التنسيق](../format-ideas/)\n"}}))
        .expect("save");
    ask(&mut h, "ui.open", json!({"path": page})).expect("open");
    settle(&mut h);

    // The painted line carries its words, so it is found by them, the way a reader hears it.
    let rect = h.get_by_label("صفحة التنسيق").rect();
    // Such a line hangs from the right edge of the page, so that is where its words are.
    ask(&mut h, "ui.click", json!({"x": rect.max.x - 24.0, "y": rect.center().y})).expect("click");
    for _ in 0..6 {
        pump(&mut h);
    }
    assert!(h.state().body().is_some_and(|b| b.starts_with("# Format ideas")), "the linked page is open, not {:?}", h.state().body());
}

/// A click lands on the word under the pointer even when a link sits earlier in the line.
/// The words on the page are shorter than the text the caret writes in by every mark and
/// every link target, so measuring the pointer against the written text would put the caret
/// a long way back - far enough, here, to land inside the link and open it by accident.
#[test]
fn the_caret_lands_in_the_last_word_of_a_line_that_holds_a_link() {
    let (_t, app, s) = sample_app("ui-caret-after-link");
    let mut h = harness(app, LIGHT);
    h.run();
    let ideas = s.md_page.parent().expect("section").to_string_lossy().into_owned();
    let made = ask(
        &mut h,
        "engine.execute",
        json!({"command": "node.create", "params": {"parent": ideas, "kind": "page", "title": "After a link", "format": "cmgfm"}}),
    )
    .expect("page");
    let page = made.as_str().expect("a path").to_string();
    let body = "see [the format page](../format-ideas/) and stop\n";
    ask(&mut h, "engine.execute", json!({"command": "page.save", "params": {"path": page, "body": body}})).expect("save");
    ask(&mut h, "ui.open", json!({"path": page})).expect("open");
    settle(&mut h);

    // On the page the marks are gone, so the line is found by the words a reader sees.
    let rect = h.get_by_label("see the format page and stop").rect();
    ask(&mut h, "ui.click", json!({"x": rect.max.x - 6.0, "y": rect.center().y})).expect("click");
    for _ in 0..6 {
        pump(&mut h);
    }
    assert!(h.state().body().is_some_and(|b| b.starts_with("see [the format page]")), "the page stays open: {:?}", h.state().body());
    let at = ask(&mut h, "ui.inspect", json!({})).expect("inspect").pointer("/page/caret/at").and_then(Value::as_u64).unwrap_or(0) as usize;
    let last_word = body.find("stop").unwrap_or(0);
    assert!(at >= last_word, "the caret is in the last word: character {at} of {:?}, which begins at {last_word}", body.trim_end());
}

/// The Welcome notebook: the File screen adds it to an empty folder, the window opens on its
/// start page, the link to the English group opens that group's first page, and asking
/// again opens the one that is there instead of making another.
#[test]
fn the_welcome_notebook_is_added_once_and_its_links_work() {
    let t = TempDir::new("ui-welcome");
    let app = NoteCraftApp::new(session(&t));
    let mut h = harness(app, LIGHT);
    h.run();
    assert!(h.state().session().tree().is_empty());
    h.state_mut().add_welcome();
    settle(&mut h);
    assert_eq!(h.state().session().tree().len(), 1);
    assert_eq!(h.state().session().tree()[0].info.title, "Welcome to NoteCraft");
    assert_eq!(h.state().status(), "Updated the welcome notebook");
    // The start page's title is in its head and the window shows it above the page. Its
    // large banner is the first thing in the body.
    assert_eq!(h.state().title(), Some("Welcome to NoteCraft (Ver 0.0.7)"), "opened on the start page");
    assert!(
        h.state().body().is_some_and(|b| b.starts_with("<p><img src=\"assets/banner-start.png\"")),
        "the first page is bundled as HTML: {:?}",
        h.state().body()
    );
    review(&mut h, "welcome_start");

    // The start page links to the English group, and the group opens on its first page.
    // The large banner puts the link below the fold, so scroll the page to it first.
    ask(&mut h, "ui.move", json!({"x": 700, "y": 400})).expect("pointer onto the page");
    settle(&mut h);
    ask(&mut h, "ui.scroll", json!({"y": -400})).expect("scroll to the language links");
    settle(&mut h);
    review(&mut h, "welcome_language_links");
    // "English" is on screen twice, once as a row of the tree and once in the page, so the
    // one further right is the page's own, and the click lands on its first word.
    let rect =
        h.get_all_by_label_contains("English").map(|n| n.rect()).max_by(|a, b| a.min.x.total_cmp(&b.min.x)).expect("the link on the start page");
    ask(&mut h, "ui.click", json!({"x": rect.min.x + 20.0, "y": rect.center().y})).expect("click");
    for _ in 0..6 {
        pump(&mut h);
    }
    assert_eq!(h.state().title(), Some("Welcome"), "the English welcome page");
    assert!(h.state().body().is_some_and(|b| b.contains("banner-en.png")), "which opens with its banner: {:?}", h.state().body());

    h.state_mut().add_welcome();
    settle(&mut h);
    assert_eq!(h.state().session().tree().len(), 1, "no second copy");
    assert_eq!(h.state().status(), "Updated the welcome notebook");
}

/// The same thing over a real loopback connection, with the key file.
#[test]
fn the_control_port_works_end_to_end() {
    let t = TempDir::new("ui-port");
    let probe = harness(NoteCraftApp::new(session(&t)), LIGHT);
    let (server, rx) = start(0, t.path(), probe.ctx.clone()).expect("start");
    let (port, key) = (server.port, std::fs::read_to_string(&server.key_file).expect("key file"));
    drop(probe);
    let mut app = NoteCraftApp::new(session(&t));
    app.set_redact_paths(true);
    let mut h = harness(app.with_control(server, rx), LIGHT);
    let client = std::thread::spawn(move || {
        use std::io::{BufRead, BufReader, Write};
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).expect("connect");
        s.set_read_timeout(Some(Duration::from_secs(20))).expect("timeout");
        let mut r = BufReader::new(s.try_clone().expect("clone"));
        let mut replies = Vec::new();
        for line in [
            json!({"id": 1, "key": key, "method": "engine.execute", "params": {"command": "notebook.create", "params": {"name": "Over TCP"}}}),
            json!({"id": 2, "method": "ui.inspect"}),
        ] {
            writeln!(s, "{line}").expect("write");
            let mut reply = String::new();
            r.read_line(&mut reply).expect("reply");
            replies.push(serde_json::from_str::<Value>(&reply).expect("json"));
        }
        replies
    });
    for _ in 0..400 {
        pump(&mut h);
        std::thread::sleep(Duration::from_millis(10));
        if client.is_finished() {
            break;
        }
    }
    let replies = client.join().expect("client");
    assert_eq!(replies[0]["ok"], true, "{replies:?}");
    assert_eq!(replies[1]["result"]["rows"].as_array().map(Vec::len), Some(1));
    assert_eq!(replies[1]["result"]["rows"][0]["title"], "Over TCP");
}

// ---------- pixels ----------

#[test]
fn display_math_paints_ink_inside_its_box() {
    let mut h = Harness::builder().with_size(egui::vec2(400.0, 160.0)).wgpu().build_ui(|ui| {
        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, egui::Color32::WHITE);
        notecraft_ui_egui::fonts::install_once(ui.ctx());
        notecraft_ui_egui::mathview::display(ui, "x = \\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}", 28.0, egui::Color32::BLACK);
    });
    h.run();
    let img = h.render().expect("render");
    let dark: Vec<(u32, u32)> =
        img.enumerate_pixels().filter(|(_, _, p)| p.0[0] < 100 && p.0[1] < 100 && p.0[2] < 100).map(|(x, y, _)| (x, y)).collect();
    assert!(dark.len() > 300, "the formula should leave ink, got {} dark pixels", dark.len());
    let (minx, maxx) = (dark.iter().map(|d| d.0).min().unwrap_or(0), dark.iter().map(|d| d.0).max().unwrap_or(0));
    assert!(maxx - minx > 120, "the formula is wide: {minx}..{maxx}");
    assert!(maxx < img.width() && dark.iter().all(|d| d.1 < img.height()));
}

#[test]
fn an_equation_in_a_page_is_not_shown_as_tex_source() {
    let (_t, mut app, s) = sample_app("ui-eq");
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    // The preview draws the formula; the old placeholder label must be gone.
    assert!(h.query_by_label_contains("Equation (TeX)").is_none(), "the equation placeholder label is gone");
    review(&mut h, "equations");
}

// ---------- themes ----------

#[test]
fn the_theme_menu_offers_fourteen_themes() {
    let t = TempDir::new("ui-menu");
    let mut h = harness(NoteCraftApp::new(session(&t)), DARK);
    h.run();
    h.get_by_label("Theme: Dark Default").click();
    h.run();
    assert_eq!(h.query_all_by_role(egui::accesskit::Role::RadioButton).count(), 14);
    review(&mut h, "theme_menu");
}

#[test]
fn picking_a_swatch_changes_the_theme_and_is_remembered() {
    let t = TempDir::new("ui-pick");
    let mut h = harness(NoteCraftApp::new(session(&t)), LIGHT);
    h.run();
    h.get_by_label("Theme: Light Default").click();
    h.run();
    h.get_by_label("Theme Dark Purple").click();
    h.run();
    assert_eq!(h.state().theme(), Theme { mode: Mode::Dark, accent: Accent::Purple });
    assert_eq!(h.state().mode_pref(), ModePref::Dark);
    drop(h);
    let again = NoteCraftApp::new(session(&t));
    assert_eq!(again.mode_pref(), ModePref::Dark, "a new window starts in the saved theme");
    let mut h2 = Harness::builder().with_size(egui::vec2(1200.0, 760.0)).build_ui_state(|ui, a: &mut NoteCraftApp| a.show(ui), again);
    h2.run();
    assert_eq!(h2.state().theme(), Theme { mode: Mode::Dark, accent: Accent::Purple });
}

/// A swatch names a mode, so clicking one means that mode even while the window is following
/// the system. Before this was fixed, picking the opposite mode changed only the accent and
/// the window kept the system's mode: "Dark Default" on a light desktop looked light.
#[test]
fn a_swatch_overrides_the_system_mode() {
    let t = TempDir::new("ui-swatch-mode");
    let mut app = NoteCraftApp::new(session(&t));
    app.follow_system_theme(Accent::Default);
    assert_eq!(app.mode_pref(), ModePref::System);
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 760.0)).build_ui_state(|ui, a: &mut NoteCraftApp| a.show(ui), app);
    h.run();
    // The harness reports a dark system, so the swatch to prove the point is the light one.
    let system = h.state().theme().mode;
    let (other, label) = if system == Mode::Dark { (Mode::Light, "Theme Light Default") } else { (Mode::Dark, "Theme Dark Default") };
    h.get_by_label(&format!("Theme: {}", Theme { mode: system, accent: Accent::Default }.label())).click();
    h.run();
    h.get_by_label(label).click();
    h.run();
    assert_eq!(h.state().theme(), Theme { mode: other, accent: Accent::Default });
    assert_eq!(h.state().mode_pref(), if other == Mode::Dark { ModePref::Dark } else { ModePref::Light }, "the swatch stops following the system");
    // And the window really is painted in that mode.
    let img = h.render().expect("render");
    let total = img.pixels().count();
    let matching = img
        .pixels()
        .filter(|p| {
            let l = notecraft_ui_egui::theme::luminance(egui::Color32::from_rgb(p[0], p[1], p[2]));
            if other == Mode::Dark { l < 0.1 } else { l > 0.6 }
        })
        .count();
    assert!(matching * 2 > total, "the window is not painted in the picked mode: {matching} of {total} pixels");
}

/// The ribbon's Theme button opens the same chooser as the sun or moon, and picking a
/// swatch there changes the theme.
#[test]
fn the_ribbon_theme_button_opens_the_chooser() {
    let (_t, app, _s) = sample_app("ui-ribbon-theme");
    let mut h = harness(app, LIGHT);
    h.run();
    // Appearance is the last group of the View tab, so in a narrow window it is past the
    // fold and the ribbon has to be scrolled to reach it. The window is widened instead,
    // which is what a person would do; the sun in the top bar opens the same chooser at
    // any width, and `the_ribbon_scrolls_rather_than_cutting_a_group_off` covers the fold.
    ask(&mut h, "ui.resize", json!({"width": 1700.0, "height": 760.0})).expect("wider");
    settle(&mut h);
    h.get_by_label("View").click();
    h.run();
    h.get_by_label("Theme").click();
    h.run();
    assert!(h.query_all_by_role(egui::accesskit::Role::RadioButton).count() >= 14, "the 14 swatches are on the screen");
    h.get_by_label("Theme Dark Green").click();
    h.run();
    assert_eq!(h.state().theme(), Theme { mode: Mode::Dark, accent: Accent::Green });
    review(&mut h, "ribbon_theme");
}

/// The Draw tab end to end: pick a pen off the rack, write on the page itself, and the
/// strokes are in `ink.svg` in the page's own folder with the pen's readings in them. Then
/// rub one out and take the other back.
#[test]
fn ink_is_written_on_the_page_itself_and_saved_beside_it() {
    let (_t, mut app, s) = sample_app("ui-draw");
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    h.get_by_label("Draw").click();
    h.run();

    // Every group OneNote's Draw tab has is on the screen, built or honestly labelled.
    for group in ["Undo", "Selection", "Drawing Tools", "Input Mode", "Stencils", "Edit", "Shapes", "Text", "Math", "Replay", "Mode", "Help"] {
        assert!(h.query_all_by_label(group).count() >= 1, "the {group} group is there");
    }
    for pen in ["Pen black", "Pen red", "Pen blue", "Pen green", "Pencil grey", "Highlighter yellow", "Highlighter mint"] {
        assert!(h.query_all_by_label(pen).count() >= 1, "{pen} is on the rack");
    }

    // A page nobody has drawn on has no ink file at all.
    let file = s.md_page.join("ink.svg");
    assert!(!file.exists(), "a clean page keeps no empty ink file");

    // Pick the red pen and write across the middle of the page. No sheet is inserted
    // first: the page itself takes the ink, the way OneNote's does.
    h.get_by_label("Pen red").click();
    h.run();
    let body = h.state().page_rect().expect("the page is on the screen");
    let y = body.min.y + 160.0;
    ask(&mut h, "ui.press", json!({"x": body.min.x + 40.0, "y": y})).expect("down");
    pump(&mut h);
    for step in 1..=6 {
        ask(&mut h, "ui.move", json!({"x": body.min.x + 40.0 + step as f32 * 30.0, "y": y})).expect("move");
        pump(&mut h);
    }
    ask(&mut h, "ui.release", json!({"x": body.min.x + 220.0, "y": y})).expect("up");
    pump(&mut h);
    // Wait for the stroke to land rather than assuming one settle is enough: under the
    // load of the whole suite the frames a synthetic drag needs do not always arrive in
    // the same number of steps, and this test failed once that way and passed alone.
    let mut ink = Value::Null;
    for _ in 0..20 {
        settle(&mut h);
        ink = ask(&mut h, "ui.inspect", json!({})).expect("inspect")["ink"].clone();
        if ink["strokes"].as_u64() == Some(1) {
            break;
        }
    }

    assert_eq!(ink["strokes"].as_u64(), Some(1), "the window says one stroke: {ink}");
    assert_eq!(ink["colour"].as_str(), Some("#d02121"), "in red");
    let written = std::fs::read_to_string(&file).expect("the ink was written beside the page");
    assert!(written.contains("nc:dabs"), "the pen's own readings are in the file:\n{written}");
    assert!(written.contains("fill=\"#d02121\""), "drawn in the red that was picked");
    assert_eq!(written.matches("<path").count(), 1, "one stroke");

    // The page's words say it has ink, so the source and the page agree and another
    // program opening the folder can see the handwriting.
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(body.starts_with("# Format ideas"), "the words are as they were: {body}");
    assert!(
        body.contains("![Ink](ink.svg)"),
        "and the page names its ink:
{body}"
    );

    // Undo takes the stroke back, and with nothing left the file goes too.
    h.get_by_label("Undo stroke").click();
    settle(&mut h);
    assert!(!file.exists(), "a page rubbed clean keeps no empty ink file");
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(
        !body.contains("ink.svg"),
        "and stops claiming to have ink:
{body}"
    );
    review(&mut h, "draw_tab");
}

/// Page ink crosses container edges and stays visible in the open space between them.
#[test]
fn page_ink_is_visible_across_container_borders() {
    let (_t, mut h, _, (ox, oy)) = two_boxes("ui-ink-across-boxes");
    let boxes = ask(&mut h, "ui.inspect", json!({})).expect("inspect arranged page");
    assert_eq!(
        boxes.pointer("/page/containers").and_then(Value::as_array).map(Vec::len),
        Some(2),
        "the stroke crosses two real page containers: {boxes}"
    );
    h.get_by_label("Draw").click();
    h.run();
    h.get_by_label("Pen red").click();
    h.run();

    // Carry the stroke beyond the second box's right edge too. Checking only the
    // gap and the second box would miss a clip rect that ends at that box.
    let (y, start, end) = (oy + 60.0, ox + 200.0, ox + 740.0);
    ask(&mut h, "ui.press", json!({"x": start, "y": y})).expect("press in the first box");
    pump(&mut h);
    for x in [ox + 280.0, ox + 340.0, ox + 430.0, end] {
        ask(&mut h, "ui.move", json!({"x": x, "y": y})).expect("draw across the container edge");
        pump(&mut h);
    }
    ask(&mut h, "ui.release", json!({"x": end, "y": y})).expect("release in the second box");
    settle(&mut h);

    let ink = ask(&mut h, "ui.inspect", json!({})).expect("inspect")["ink"].clone();
    assert_eq!(ink["strokes"].as_u64(), Some(1), "the pen made one page stroke: {ink}");
    let image = h.render().expect("render the page with the stroke");
    review(&mut h, "ink_across_boxes");
    let scale = image.width() as f64 / 1200.0;
    let visible_red_near = |x: f64| {
        let (cx, cy) = ((x * scale).round() as i32, (y * scale).round() as i32);
        (-4..=4).any(|dy| {
            (-4..=4).any(|dx| {
                let (px, py) = (cx + dx, cy + dy);
                px >= 0 && py >= 0 && (px as u32) < image.width() && (py as u32) < image.height() && {
                    let pixel = image.get_pixel(px as u32, py as u32);
                    pixel[0] > 170 && pixel[1] < 90 && pixel[2] < 90
                }
            })
        })
    };
    for x in [ox + 280.0, ox + 340.0, ox + 430.0, ox + 700.0] {
        assert!(visible_red_near(x), "red page ink is visible at x={x}, including across the box edge");
    }
}

#[test]
fn page_ink_crosses_a_diagram_container_edge() {
    let (_t, mut app, sample) = sample_app("ui-ink-diagram-edge");
    let page = sample.md_page.to_string_lossy().to_string();
    let md_file = sample.md_page.join("index.md");
    let demo_page = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/demo/DemoNotebook/cmgfm/10-en/how-to/mermaid-diagrams/index.md");
    let source = std::fs::read_to_string(demo_page).expect("read the Mermaid diagram demo page");
    std::fs::write(&md_file, source).expect("write diagram page");
    app.open_path(&page);
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    let info = ask(&mut h, "ui.inspect", json!({})).expect("inspect page");
    let blocks = info.pointer("/page/blocks").and_then(Value::as_u64).unwrap_or(0);
    assert!(blocks >= 3, "the test page has its flowchart: {info}");
    ask(&mut h, "engine.execute", json!({"command": "page.containers", "params": {"path": page.clone(), "boxes": format!("200,0,360,{blocks}")}}))
        .expect("put the diagram in a narrow container");
    ask(&mut h, "ui.open", json!({"path": page})).expect("open arranged page");
    settle(&mut h);
    let arranged = ask(&mut h, "ui.inspect", json!({})).expect("inspect arranged page");
    assert_eq!(
        arranged.pointer("/page/containers/0/x").and_then(Value::as_f64),
        Some(200.0),
        "the container begins to the right of open paper: {arranged}"
    );
    assert_eq!(
        arranged.pointer("/page/containers/0/width").and_then(Value::as_f64),
        Some(360.0),
        "the flowchart is inside a 360 point container: {arranged}"
    );
    h.get_by_label("Draw").click();
    pump(&mut h);
    h.get_by_label("Pen red").click();
    pump(&mut h);
    let info = ask(&mut h, "ui.inspect", json!({})).expect("inspect arranged page");
    let origin = info.pointer("/page/origin").and_then(Value::as_array).expect("page origin");
    let (ox, oy) = (origin[0].as_f64().unwrap_or(0.0), origin[1].as_f64().unwrap_or(0.0));
    let (y, start, end) = (oy + 190.0, ox + 450.0, ox + 100.0);
    ask(&mut h, "ui.press", json!({"x": start, "y": y})).expect("press over diagram");
    pump(&mut h);
    for x in [ox + 350.0, ox + 250.0, end] {
        ask(&mut h, "ui.move", json!({"x": x, "y": y})).expect("draw across diagram edge");
        pump(&mut h);
    }
    ask(&mut h, "ui.release", json!({"x": end, "y": y})).expect("release outside the container");
    settle(&mut h);
    // Repeat in the opposite direction: begin on open paper and draw into the box.
    let y2 = y + 35.0;
    ask(&mut h, "ui.press", json!({"x": ox + 100.0, "y": y2})).expect("press on open paper");
    pump(&mut h);
    for x in [ox + 200.0, ox + 350.0, ox + 450.0] {
        ask(&mut h, "ui.move", json!({"x": x, "y": y2})).expect("draw into the diagram container");
        pump(&mut h);
    }
    ask(&mut h, "ui.release", json!({"x": ox + 450.0, "y": y2})).expect("release inside the container");
    settle(&mut h);
    let ink = ask(&mut h, "ui.inspect", json!({})).expect("inspect strokes")["ink"].clone();
    assert_eq!(ink["strokes"].as_u64(), Some(2), "both strokes remain on the page: {ink}");
    let image = h.render().expect("render diagram and ink");
    review(&mut h, "ink_across_diagram_container");
    let scale = image.width() as f64 / 1200.0;
    let visible_red = |x: f64, at_y: f64| {
        let (cx, cy) = ((x * scale).round() as i32, (at_y * scale).round() as i32);
        (-4..=4).any(|dy| {
            (-4..=4).any(|dx| {
                let (px, py) = (cx + dx, cy + dy);
                px >= 0 && py >= 0 && (px as u32) < image.width() && (py as u32) < image.height() && {
                    let pixel = image.get_pixel(px as u32, py as u32);
                    pixel[0] > 170 && pixel[1] < 90 && pixel[2] < 90
                }
            })
        })
    };
    assert!(visible_red(ox + 150.0, y), "ink is visible in open page space past the diagram container's left edge");
    assert!(visible_red(ox + 150.0, y2), "ink remains visible when a stroke starts on open paper and enters the container");
}

/// The same on an HTML page, and the ink comes back when the page is opened again.
#[test]
fn ink_comes_back_when_the_page_is_opened_again() {
    let (_t, mut app, s) = sample_app("ui-draw-html");
    app.open_path(&s.html_page.to_string_lossy());
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    h.get_by_label("Draw").click();
    h.run();
    h.get_by_label("Pen blue").click();
    h.run();

    let body = h.state().page_rect().expect("the page is on the screen");
    let y = body.min.y + 140.0;
    ask(&mut h, "ui.press", json!({"x": body.min.x + 30.0, "y": y})).expect("down");
    pump(&mut h);
    for step in 1..=4 {
        ask(&mut h, "ui.move", json!({"x": body.min.x + 30.0 + step as f32 * 25.0, "y": y + step as f32 * 4.0})).expect("move");
        pump(&mut h);
    }
    ask(&mut h, "ui.release", json!({"x": body.min.x + 130.0, "y": y + 16.0})).expect("up");
    settle(&mut h);
    assert!(s.html_page.join("ink.svg").exists(), "the ink is beside the HTML page too");

    // Away to another page and back: the strokes are strokes again, not a picture.
    ask(&mut h, "ui.open", json!({"path": &s.md_page.to_string_lossy()})).expect("another page");
    settle(&mut h);
    assert_eq!(ask(&mut h, "ui.inspect", json!({})).expect("inspect")["ink"]["strokes"].as_u64(), Some(0), "the other page has no ink of its own");
    ask(&mut h, "ui.open", json!({"path": &s.html_page.to_string_lossy()})).expect("back");
    settle(&mut h);
    let ink = ask(&mut h, "ui.inspect", json!({})).expect("inspect")["ink"].clone();
    assert_eq!(ink["strokes"].as_u64(), Some(1), "the stroke came back as a stroke: {ink}");

    // Saved as a plain picture it is beside the ink and holds no readings.
    h.get_by_label("Save picture").click();
    settle(&mut h);
    let plain = s.html_page.join("ink-picture.svg");
    assert!(plain.exists(), "the picture was written");
    assert!(!std::fs::read_to_string(&plain).unwrap_or_default().contains("nc:"), "and holds no readings");
}

/// Every tool on the rack can be picked up. This is the test that was missing when a
/// second interact was laid over each pen for its right-click menu: later widgets win in
/// egui, so the left click was swallowed and only the eraser, which had no such overlay,
/// could be chosen. Nothing noticed, because nothing clicked a pen.
#[test]
fn every_tool_on_the_rack_can_be_picked_up() {
    let (_t, mut app, s) = sample_app("ui-rack-pick");
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    h.get_by_label("Draw").click();
    h.run();

    let ink = |h: &mut Harness<'_, NoteCraftApp>| ask(h, "ui.inspect", json!({})).expect("inspect")["ink"].clone();
    for (pen, tool, colour) in [
        ("Pen red", "pen", "#d02121"),
        ("Highlighter yellow", "highlighter", "#ffdd33"),
        ("Pencil grey", "pencil", "#555555"),
        ("Pen blue", "pen", "#1855c4"),
        ("Pen black", "pen", "#202020"),
        ("Pen green", "pen", "#167f3d"),
        ("Highlighter mint", "highlighter", "#6fe39a"),
    ] {
        h.get_by_label(pen).click();
        h.run();
        let now = ink(&mut h);
        assert_eq!(now["pointer"].as_str(), Some("draw"), "{pen} put the pen in hand: {now}");
        assert_eq!(now["tool"].as_str(), Some(tool), "{pen} is a {tool}: {now}");
        assert_eq!(now["colour"].as_str(), Some(colour), "{pen} writes in {colour}: {now}");
    }

    // The eraser, the arrow and Type each take the pen out of the hand.
    h.get_by_label("Eraser").click();
    h.run();
    assert_eq!(ink(&mut h)["pointer"].as_str(), Some("erase"));
    h.get_by_label("Select").click();
    h.run();
    assert_eq!(ink(&mut h)["pointer"].as_str(), Some("select"), "the arrow picks ink up");
    h.get_by_label("Type").click();
    h.run();
    assert_eq!(ink(&mut h)["pointer"].as_str(), Some("write"), "and Type puts the caret back");
}

/// The rack is the owner's to arrange: another tool added, moved along, changed and taken
/// off, and still there the next time the window opens.
#[test]
fn the_rack_can_be_arranged_and_is_remembered() {
    let t = TempDir::new("ui-rack-edit");
    let mut sess = session(&t);
    let s = create_sample(&mut sess, Some(LOGO_PNG)).expect("sample");
    let mut app = NoteCraftApp::new(sess);
    app.set_redact_paths(true);
    app.open_path(&s.md_page.to_string_lossy());
    let settings = t.path().join("settings.toml");
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    h.get_by_label("Draw").click();
    h.run();
    let rack =
        |h: &mut Harness<'_, NoteCraftApp>| ask(h, "ui.inspect", json!({})).expect("inspect")["ink"]["rack"].as_str().unwrap_or_default().to_string();
    let before = rack(&mut h);
    assert_eq!(before.split('|').count(), 7, "the rack a notebook starts with: {before}");

    // Add a fountain pen: it goes on the end and comes to hand.
    h.get_by_label("Add").click();
    h.run();
    h.run();
    for probe in ["Add", "Pen", "Pencil", "Fountain Pen", "Add Fountain Pen", "Add Pen", "Laser Pointer"] {
        eprintln!("LABELS {probe}: {}", h.query_all_by_label(probe).count());
    }
    h.get_by_label("Add Fountain Pen").click();
    settle(&mut h);
    let after = rack(&mut h);
    assert_eq!(after.split('|').count(), 8, "one more tool: {after}");
    assert!(after.split('|').next_back().is_some_and(|n| n.starts_with("fountain-pen:")), "on the end: {after}");
    let now = ask(&mut h, "ui.inspect", json!({})).expect("inspect")["ink"].clone();
    assert_eq!(now["tool"].as_str(), Some("fountain-pen"), "and in hand: {now}");
    assert_eq!(now["nib"].as_u64(), Some(7));

    // It was written down, so the next window opens with it.
    let saved = std::fs::read_to_string(&settings).unwrap_or_default();
    assert!(
        saved.contains("fountain-pen"),
        "the rack is in the settings file:
{saved}"
    );
    let again = NoteCraftApp::new(session(&t));
    let mut h2 = Harness::builder().with_size(egui::vec2(1200.0, 760.0)).build_ui_state(|ui, a: &mut NoteCraftApp| a.show(ui), again);
    h2.run();
    let reopened = ask(&mut h2, "ui.inspect", json!({})).expect("inspect")["ink"]["rack"].as_str().unwrap_or_default().to_string();
    assert_eq!(reopened, after, "the rack came back as it was left");
}

/// Ink can be picked up, moved and thrown away, and a lasso takes what is wholly inside
/// it. The strokes are their own objects on the page: nothing about the words moves them.
#[test]
fn ink_can_be_picked_up_moved_and_deleted() {
    let (_t, mut app, s) = sample_app("ui-pick");
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    h.get_by_label("Draw").click();
    h.run();
    h.get_by_label("Pen black").click();
    h.run();

    // Two strokes, well apart.
    let body = h.state().page_rect().expect("the page");
    let draw = |h: &mut Harness<'_, NoteCraftApp>, x: f32, y: f32| {
        ask(h, "ui.press", json!({"x": x, "y": y})).expect("down");
        pump(h);
        for step in 1..=4 {
            ask(h, "ui.move", json!({"x": x + step as f32 * 20.0, "y": y})).expect("move");
            pump(h);
        }
        ask(h, "ui.release", json!({"x": x + 80.0, "y": y})).expect("up");
        pump(h);
    };
    draw(&mut h, body.min.x + 40.0, body.min.y + 120.0);
    draw(&mut h, body.min.x + 40.0, body.min.y + 260.0);
    let ink = |h: &mut Harness<'_, NoteCraftApp>| ask(h, "ui.inspect", json!({})).expect("inspect")["ink"].clone();
    let mut now = Value::Null;
    for _ in 0..20 {
        settle(&mut h);
        now = ink(&mut h);
        if now["strokes"].as_u64() == Some(2) {
            break;
        }
    }
    assert_eq!(now["strokes"].as_u64(), Some(2), "two strokes on the page: {now}");

    // Select, then click the first one: it is held, and only it.
    h.get_by_label("Select").click();
    pump(&mut h);
    ask(&mut h, "ui.click", json!({"x": body.min.x + 80.0, "y": body.min.y + 120.0})).expect("pick");
    settle(&mut h);
    assert_eq!(ink(&mut h)["holding"].as_u64(), Some(1), "one stroke is held: {}", ink(&mut h));

    // Clicking bare page lets go again.
    ask(&mut h, "ui.click", json!({"x": body.min.x + 300.0, "y": body.min.y + 60.0})).expect("let go");
    settle(&mut h);
    assert_eq!(ink(&mut h)["holding"].as_u64(), Some(0), "and bare page lets go");

    // A lasso round the lower stroke takes that one and leaves the upper one alone.
    h.get_by_label("Lasso Select").click();
    pump(&mut h);
    let (lx, ly) = (body.min.x + 20.0, body.min.y + 230.0);
    ask(&mut h, "ui.press", json!({"x": lx, "y": ly})).expect("down");
    pump(&mut h);
    for (dx, dy) in [(180.0, 0.0), (180.0, 70.0), (0.0, 70.0), (0.0, 2.0)] {
        ask(&mut h, "ui.move", json!({"x": lx + dx, "y": ly + dy})).expect("move");
        pump(&mut h);
    }
    ask(&mut h, "ui.release", json!({"x": lx, "y": ly + 2.0})).expect("up");
    settle(&mut h);
    assert_eq!(ink(&mut h)["holding"].as_u64(), Some(1), "the lasso took the one inside it: {}", ink(&mut h));

    // Delete throws the held stroke away, and the other one stays.
    ask(&mut h, "ui.key", json!({"key": "Delete"})).expect("delete");
    settle(&mut h);
    let after = ink(&mut h);
    assert_eq!(after["strokes"].as_u64(), Some(1), "one stroke left: {after}");
    assert_eq!(after["holding"].as_u64(), Some(0), "and nothing is held");
}

/// The Insert tab, group for group as OneNote has it, with the stamps it can really put
/// on a page actually putting them there.
#[test]
fn the_insert_tab_has_onenotes_groups_and_its_stamps_work() {
    let (_t, mut app, s) = sample_app("ui-insert");
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    h.get_by_label("Insert").click();
    h.run();

    for group in ["Insert", "Tables", "Files", "Images", "Media", "Links", "Recording", "Time Stamp", "Pages", "Symbols", "More"] {
        assert!(h.query_all_by_label(group).count() >= 1, "the {group} group is there");
    }
    for control in ["Table", "File Attachment", "Pictures", "Link", "Date", "Time", "Date & Time", "Equation", "Diagram"] {
        assert!(h.query_all_by_label(control).count() >= 1, "{control} is on the tab");
    }

    // The three stamps each put something different on the page.
    ask(&mut h, "ui.caret", json!({"block": 1})).expect("caret");
    pump(&mut h);
    h.get_by_label("Time").click();
    settle(&mut h);
    let body = h.state().body().unwrap_or_default().to_string();
    // A clock, five characters of it: two digits, a colon, two digits.
    let clock: Vec<&str> = body.lines().filter(|l| l.len() >= 5 && l.contains(':')).collect();
    assert!(
        !clock.is_empty(),
        "the time went in:
{body}"
    );

    h.get_by_label("Date").click();
    settle(&mut h);
    let body = h.state().body().unwrap_or_default().to_string();
    let months = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    assert!(
        months.iter().any(|m| body.contains(m)),
        "the date went in as words:
{body}"
    );
}

/// The Equation button opens a list of equations, each drawn as it will look, and picking one
/// puts its TeX in the page.
#[test]
fn the_equation_button_offers_equations_and_puts_the_one_picked_in_the_page() {
    let (_t, mut app, s) = sample_app("ui-eq-gallery");
    app.open_path(&s.md_page.to_string_lossy());
    // Wide enough that the Symbols group is on the screen and not scrolled out of the ribbon.
    let mut h = sized_harness(app, LIGHT, egui::vec2(2000.0, 900.0));
    settle(&mut h);
    h.get_by_label("Insert").click();
    h.run();
    ask(&mut h, "ui.caret", json!({"block": 1})).expect("caret");
    pump(&mut h);

    // Clicked with a pointer position, as a person does: the list closes on a click elsewhere,
    // and it asks where the pointer is.
    let button = h.get_by_label("Equation").rect();
    ask(&mut h, "ui.click", json!({"x": button.center().x, "y": button.center().y})).expect("click the button");
    for _ in 0..8 {
        pump(&mut h);
    }
    for (name, _) in notecraft_ui_egui::ribbon::EQUATIONS {
        assert!(h.query_all_by_label(&format!("Equation: {name}")).count() >= 1, "{name} is in the list");
    }
    let row = h.get_by_label("Equation: Matrix").rect();
    ask(&mut h, "ui.click", json!({"x": row.center().x, "y": row.center().y})).expect("pick the matrix");
    for _ in 0..8 {
        pump(&mut h);
    }
    settle(&mut h);
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(body.contains(r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}"), "the matrix went into the page:\n{body}");
    assert_eq!(h.query_all_by_label("Equation: Matrix").count(), 0, "the list closed after a pick");
}

/// The page has a menu under the right button, with the commands a page needs: paste,
/// cut, copy, and the things the Insert tab can put there.
#[test]
fn the_page_has_a_right_click_menu() {
    let (_t, mut app, s) = sample_app("ui-page-menu");
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    let body = h.state().page_rect().expect("the page");
    ask(&mut h, "ui.click", json!({"x": body.min.x + 300.0, "y": body.min.y + 40.0, "button": "right"})).expect("right click");
    for _ in 0..6 {
        pump(&mut h);
    }
    // Named apart from the Home tab's own Paste, Cut, Copy and Link, which are always on
    // the screen: these are the ones the page's menu adds.
    for item in ["Table", "Equation", "Date and time", "Picture from a file", "Attach a file"] {
        assert!(h.query_all_by_label(item).count() >= 1, "{item} is on the page's menu");
    }
    for item in ["Paste", "Cut", "Copy"] {
        assert!(h.query_all_by_label(item).count() >= 2, "{item} is on the menu as well as the ribbon");
    }

    // Picking one does what it says: a table goes into the page.
    h.get_by_label("Table").click();
    settle(&mut h);
    let page = h.state().body().unwrap_or_default().to_string();
    assert!(
        page.contains("---"),
        "a table went in:
{page}"
    );
}

/// A link inside a list item, which is how the Welcome pages are written. The owner
/// found that clicking one did nothing in English and showed the raw markup in Arabic,
/// where every earlier link test used a plain paragraph.
#[test]
fn a_link_in_a_list_item_opens_the_page_it_names() {
    let (_t, app, s) = sample_app("ui-link-item");
    let mut h = harness(app, LIGHT);
    h.run();
    let ideas = s.md_page.parent().expect("section").to_string_lossy().into_owned();
    let made = ask(
        &mut h,
        "engine.execute",
        json!({"command": "node.create", "params": {"parent": ideas, "kind": "page", "title": "Listed links", "format": "cmgfm"}}),
    )
    .expect("page");
    let page = made.as_str().expect("a path").to_string();
    let body = "- [Open the format page](../format-ideas/): what it says
- plain item
";
    ask(&mut h, "engine.execute", json!({"command": "page.save", "params": {"path": page, "body": body}})).expect("save");
    ask(&mut h, "ui.open", json!({"path": page})).expect("open");
    settle(&mut h);

    let rect = h.get_by_label("Open the format page: what it says").rect();
    // The link is the first words of the item, after the bullet.
    ask(&mut h, "ui.click", json!({"x": rect.min.x + 60.0, "y": rect.center().y})).expect("click");
    for _ in 0..6 {
        pump(&mut h);
    }
    assert!(h.state().body().is_some_and(|b| b.starts_with("# Format ideas")), "the linked page opened, not {:?}", h.state().body());
}

/// A table is written in, cell by cell: click a cell, type, and the page's own file has
/// the new words in it. Tab walks along the row.
#[test]
fn a_table_cell_is_written_in_and_the_page_keeps_it() {
    let (_t, app, s) = sample_app("ui-table-edit");
    let mut h = harness(app, LIGHT);
    h.run();
    let ideas = s.md_page.parent().expect("section").to_string_lossy().into_owned();
    let made = ask(
        &mut h,
        "engine.execute",
        json!({"command": "node.create", "params": {"parent": ideas, "kind": "page", "title": "A table", "format": "cmgfm"}}),
    )
    .expect("page");
    let page = made.as_str().expect("a path").to_string();
    ask(
        &mut h,
        "engine.execute",
        json!({"command": "page.save", "params": {"path": page, "body": "| A | B |
|---|---|
| one | two |
"}}),
    )
    .expect("save");
    ask(&mut h, "ui.open", json!({"path": page})).expect("open");
    settle(&mut h);

    // Click the cell that says "one". Every cell is its own control, so it is found by
    // what it says.
    let cell = h.get_by_label("one").rect();
    ask(&mut h, "ui.click", json!({"x": cell.center().x, "y": cell.center().y})).expect("click the cell");
    settle(&mut h);
    let caret = ask(&mut h, "ui.inspect", json!({})).expect("inspect").pointer("/page/caret").cloned().unwrap_or(Value::Null);
    assert_eq!(caret.get("text").and_then(Value::as_str), Some("one"), "the caret is in that cell: {caret}");

    ask(&mut h, "ui.text", json!({"text": " and a half"})).expect("type");
    settle(&mut h);
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(
        body.contains("one and a half"),
        "what was typed is in the page:
{body}"
    );
    assert!(
        body.contains("two"),
        "and the rest of the table is untouched:
{body}"
    );
}

/// Tab off the last cell of a table adds a row and lands in it, the way OneNote grows a
/// table as it is filled in.
#[test]
fn tab_off_the_end_of_a_table_adds_a_row() {
    let (_t, app, s) = sample_app("ui-table-grow");
    let mut h = harness(app, LIGHT);
    h.run();
    let ideas = s.md_page.parent().expect("section").to_string_lossy().into_owned();
    let made = ask(
        &mut h,
        "engine.execute",
        json!({"command": "node.create", "params": {"parent": ideas, "kind": "page", "title": "Grow", "format": "cmgfm"}}),
    )
    .expect("page");
    let page = made.as_str().expect("a path").to_string();
    ask(
        &mut h,
        "engine.execute",
        json!({"command": "page.save", "params": {"path": page, "body": "| A | B |
|---|---|
| one | two |
"}}),
    )
    .expect("save");
    ask(&mut h, "ui.open", json!({"path": page})).expect("open");
    settle(&mut h);

    // Into the last cell, then Tab: a row appears and the caret is in its first cell.
    let cell = h.get_by_label("two").rect();
    ask(&mut h, "ui.click", json!({"x": cell.center().x, "y": cell.center().y})).expect("click");
    settle(&mut h);
    ask(&mut h, "ui.key", json!({"key": "Tab"})).expect("tab");
    settle(&mut h);
    ask(&mut h, "ui.text", json!({"text": "three"})).expect("type");
    settle(&mut h);

    let body = h.state().body().unwrap_or_default().to_string();
    assert!(
        body.contains("three"),
        "the new row took what was typed:
{body}"
    );
    assert_eq!(
        body.lines().filter(|l| l.starts_with('|')).count(),
        4,
        "a heading, its rule and two rows:
{body}"
    );
}

/// Clicking bare page starts a box of its own where the pointer is, as OneNote does:
/// the words begin where you pointed, not at the end of whatever was written last.
#[test]
fn clicking_bare_page_starts_a_box_where_the_pointer_is() {
    let (_t, app, s) = sample_app("ui-click-space");
    let mut h = harness(app, LIGHT);
    h.run();
    // A short page, so there is real bare paper under the words to click on.
    let ideas = s.md_page.parent().expect("section").to_string_lossy().into_owned();
    let made = ask(
        &mut h,
        "engine.execute",
        json!({"command": "node.create", "params": {"parent": ideas, "kind": "page", "title": "Bare", "format": "cmgfm"}}),
    )
    .expect("page");
    let page = made.as_str().expect("a path").to_string();
    ask(
        &mut h,
        "engine.execute",
        json!({"command": "page.save", "params": {"path": page, "body": "one line
"}}),
    )
    .expect("save");
    ask(&mut h, "ui.open", json!({"path": page})).expect("open");
    settle(&mut h);

    let boxes = |h: &mut Harness<'_, NoteCraftApp>| {
        ask(h, "ui.inspect", json!({})).expect("inspect").pointer("/page/containers").and_then(Value::as_array).map_or(0, Vec::len)
    };
    let before = boxes(&mut h);
    let body = h.state().page_rect().expect("the page");
    ask(&mut h, "ui.click", json!({"x": body.min.x + 260.0, "y": body.min.y + 320.0})).expect("click the bare page");
    settle(&mut h);
    assert_eq!(boxes(&mut h), before + 1, "a box of its own appeared where the pointer was");

    ask(&mut h, "ui.text", json!({"text": "started here"})).expect("type");
    settle(&mut h);
    let page_text = h.state().body().unwrap_or_default().to_string();
    assert!(
        page_text.contains("started here"),
        "and it took what was typed:
{page_text}"
    );
    assert!(
        page_text.contains("one line"),
        "without disturbing what was already there:
{page_text}"
    );
}

/// The Table tab appears while the caret is in a table and goes when it leaves, and its
/// Insert and Delete really change the table.
#[test]
fn the_table_tab_comes_with_the_caret_and_changes_the_table() {
    let (_t, app, s) = sample_app("ui-table-tab");
    let mut h = harness(app, LIGHT);
    h.run();
    let ideas = s.md_page.parent().expect("section").to_string_lossy().into_owned();
    let made = ask(
        &mut h,
        "engine.execute",
        json!({"command": "node.create", "params": {"parent": ideas, "kind": "page", "title": "Shape", "format": "cmgfm"}}),
    )
    .expect("page");
    let page = made.as_str().expect("a path").to_string();
    ask(
        &mut h,
        "engine.execute",
        json!({"command": "page.save", "params": {"path": page, "body": "| A | B |
|---|---|
| one | two |
"}}),
    )
    .expect("save");
    ask(&mut h, "ui.open", json!({"path": page})).expect("open");
    settle(&mut h);

    // No caret in a table, no Table tab.
    assert_eq!(h.query_all_by_label("Table").count(), 0, "the tab is not offered while nothing is in hand");

    let cell = h.get_by_label("one").rect();
    ask(&mut h, "ui.click", json!({"x": cell.center().x, "y": cell.center().y})).expect("into the cell");
    settle(&mut h);
    assert!(h.query_all_by_label("Table").count() >= 1, "now the tab is there");

    h.get_by_label("Table").click();
    settle(&mut h);
    review(&mut h, "tab_table");
    for group in ["Select", "Delete", "Insert", "Merge", "Format", "Alignment", "Data", "Convert"] {
        assert!(h.query_all_by_label(group).count() >= 1, "the {group} group is there");
    }

    // A column to the right, then a row below: the table grows both ways.
    h.get_by_label("Insert Column Right").click();
    settle(&mut h);
    let body = h.state().body().unwrap_or_default().to_string();
    assert_eq!(
        body.lines().next().unwrap_or("").matches('|').count(),
        4,
        "three columns now:
{body}"
    );

    h.get_by_label("Insert Row Below").click();
    settle(&mut h);
    let body = h.state().body().unwrap_or_default().to_string();
    assert_eq!(
        body.lines().filter(|l| l.starts_with('|')).count(),
        4,
        "a heading, its rule and two rows:
{body}"
    );

    // And the heading row refuses to be deleted, with a word rather than silence.
    h.get_by_label("Delete Rows").click();
    settle(&mut h);
    h.get_by_label("Delete Rows").click();
    settle(&mut h);
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(
        body.contains("| A |"),
        "the heading is still there:
{body}"
    );
}

/// Dragging across cells picks the block between them: Delete empties it, and the Table
/// tab's Select buttons pick the same way.
#[test]
fn cells_dragged_across_are_picked_and_can_be_emptied() {
    let (_t, app, s) = sample_app("ui-cell-range");
    let mut h = harness(app, LIGHT);
    h.run();
    let ideas = s.md_page.parent().expect("section").to_string_lossy().into_owned();
    let made = ask(
        &mut h,
        "engine.execute",
        json!({"command": "node.create", "params": {"parent": ideas, "kind": "page", "title": "Range", "format": "cmgfm"}}),
    )
    .expect("page");
    let page = made.as_str().expect("a path").to_string();
    let table = "| A | B |\n|---|---|\n| one | two |\n| three | four |\n";
    ask(&mut h, "engine.execute", json!({"command": "page.save", "params": {"path": page, "body": table}})).expect("save");
    ask(&mut h, "ui.open", json!({"path": page})).expect("open");
    settle(&mut h);

    // Drag from the first cell of the first row to the last cell of the second.
    let from = h.get_by_label("one").rect().center();
    let to = h.get_by_label("four").rect().center();
    ask(&mut h, "ui.press", json!({"x": from.x, "y": from.y})).expect("press");
    settle(&mut h);
    ask(&mut h, "ui.move", json!({"x": to.x, "y": to.y})).expect("drag");
    settle(&mut h);
    ask(&mut h, "ui.release", json!({"x": to.x, "y": to.y})).expect("let go");
    settle(&mut h);
    review(&mut h, "cell-range");

    // Delete empties what was picked and leaves the heading alone.
    ask(&mut h, "ui.key", json!({"key": "Delete"})).expect("delete");
    settle(&mut h);
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(body.contains("| A | B |"), "the heading is untouched:\n{body}");
    for word in ["one", "two", "three", "four"] {
        assert!(!body.contains(word), "{word} was emptied with the rest of the block:\n{body}");
    }

    // And Select Rows from the Table tab picks a whole row, which Delete Rows then takes.
    ask(&mut h, "engine.execute", json!({"command": "page.save", "params": {"path": page, "body": table}})).expect("put it back");
    ask(&mut h, "ui.open", json!({"path": page})).expect("open it again");
    settle(&mut h);
    let cell = h.get_by_label("three").rect().center();
    ask(&mut h, "ui.click", json!({"x": cell.x, "y": cell.y})).expect("into the cell");
    settle(&mut h);
    h.get_by_label("Table").click();
    settle(&mut h);
    h.get_by_label("Select Rows").click();
    settle(&mut h);
    h.get_by_label("Delete Rows").click();
    settle(&mut h);
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(body.contains("one"), "the row that was not picked stayed:\n{body}");
    assert!(!body.contains("three"), "the picked row went:\n{body}");
}

/// A table's column borders are handles: a drag moves one, and the page remembers.
#[test]
fn a_column_border_is_dragged_and_the_page_keeps_the_width() {
    let (_t, app, s) = sample_app("ui-column-drag");
    let mut h = harness(app, LIGHT);
    h.run();
    let ideas = s.md_page.parent().expect("section").to_string_lossy().into_owned();
    let made = ask(
        &mut h,
        "engine.execute",
        json!({"command": "node.create", "params": {"parent": ideas, "kind": "page", "title": "Wide", "format": "cmgfm"}}),
    )
    .expect("page");
    let page = made.as_str().expect("a path").to_string();
    ask(
        &mut h,
        "engine.execute",
        json!({"command": "page.save", "params": {"path": page, "body": "| A | B |
|---|---|
| one | two |
"}}),
    )
    .expect("save");
    ask(&mut h, "ui.open", json!({"path": page})).expect("open");
    settle(&mut h);

    // The heading of the second column starts one column in, so the border between them
    // is a known distance from the first heading's left edge.
    let a = h.get_by_label("A").rect();
    let b = h.get_by_label("B").rect();
    let edge = b.min.x - 7.0;
    let y = a.center().y;

    ask(&mut h, "ui.press", json!({"x": edge, "y": y})).expect("take hold");
    settle(&mut h);
    ask(&mut h, "ui.move", json!({"x": edge + 60.0, "y": y})).expect("drag");
    settle(&mut h);
    ask(&mut h, "ui.release", json!({"x": edge + 60.0, "y": y})).expect("let go");
    settle(&mut h);
    review(&mut h, "table_columns");

    let kept = ask(&mut h, "engine.execute", json!({"command": "page.columns", "params": {"path": page}})).expect("widths");
    let kept = kept.as_str().unwrap_or_default().to_string();
    assert!(kept.starts_with("0:2"), "the first column is wider and the page knows: {kept:?}");

    // And the table is drawn to it: the second column starts further right than it did.
    let now = h.get_by_label("B").rect();
    assert!(now.min.x > b.min.x + 40.0, "the second column moved right: {} then {}", b.min.x, now.min.x);
}

/// Ctrl+Z takes back what was just done to the page, and Ctrl+Y puts it back: typing, and
/// a row taken out of a table.
#[test]
fn undo_takes_back_a_change_to_the_page_and_redo_puts_it_back() {
    let (_t, app, s) = sample_app("ui-undo");
    let mut h = harness(app, LIGHT);
    h.run();
    let ideas = s.md_page.parent().expect("section").to_string_lossy().into_owned();
    let made = ask(
        &mut h,
        "engine.execute",
        json!({"command": "node.create", "params": {"parent": ideas, "kind": "page", "title": "Back", "format": "cmgfm"}}),
    )
    .expect("page");
    let page = made.as_str().expect("a path").to_string();
    ask(
        &mut h,
        "engine.execute",
        json!({"command": "page.save", "params": {"path": page, "body": "words here

| A | B |
|---|---|
| one | two |
| three | four |
"}}),
    )
    .expect("save");
    ask(&mut h, "ui.open", json!({"path": page})).expect("open");
    settle(&mut h);

    // Nothing has been done yet, so there is nothing to take back.
    let state = ask(&mut h, "ui.inspect", json!({})).expect("inspect");
    assert_eq!(state["page"]["history"]["canUndo"], json!(false), "a page just opened has nothing to undo");

    // Type into the first line, then take it back.
    let line = h.get_by_label("words here").rect();
    ask(&mut h, "ui.click", json!({"x": line.center().x, "y": line.center().y})).expect("caret");
    settle(&mut h);
    ask(&mut h, "ui.text", json!({"text": " and more"})).expect("type");
    settle(&mut h);
    assert!(h.state().body().unwrap_or_default().contains("and more"), "the words went in");

    ask(&mut h, "ui.key", json!({"key": "Z", "ctrl": true})).expect("undo");
    settle(&mut h);
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(
        !body.contains("and more"),
        "undo took the typing back:
{body}"
    );
    assert!(
        body.contains("words here"),
        "and left what was there:
{body}"
    );

    ask(&mut h, "ui.key", json!({"key": "Y", "ctrl": true})).expect("redo");
    settle(&mut h);
    assert!(h.state().body().unwrap_or_default().contains("and more"), "redo put it back");

    // A row taken out of a table comes back too, which is the change that used to be
    // impossible to undo.
    let cell = h.get_by_label("three").rect();
    ask(&mut h, "ui.click", json!({"x": cell.center().x, "y": cell.center().y})).expect("into the cell");
    settle(&mut h);
    h.get_by_label("Table").click();
    settle(&mut h);
    h.get_by_label("Delete Rows").click();
    settle(&mut h);
    assert!(!h.state().body().unwrap_or_default().contains("three"), "the row went");

    ask(&mut h, "ui.key", json!({"key": "Z", "ctrl": true})).expect("undo the delete");
    settle(&mut h);
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(
        body.contains("three") && body.contains("four"),
        "the row came back:
{body}"
    );

    // And the page says it has steps to walk back through.
    let state = ask(&mut h, "ui.inspect", json!({})).expect("inspect");
    assert!(state["page"]["history"]["steps"].as_u64().unwrap_or(0) >= 2, "{}", state["page"]["history"]);
}

/// A file in the page's folder can be taken out again from the Source view, and
/// the window asks before anything is deleted.
#[test]
fn a_file_in_the_pages_folder_can_be_deleted_from_the_page() {
    let (t, app, s) = sample_app("ui-detach");
    let mut h = harness(app, LIGHT);
    h.run();
    let page = s.md_page.to_string_lossy().into_owned();
    let dropped = t.path().join("picture.png");
    std::fs::write(&dropped, LOGO_PNG).expect("a file to attach");
    ask(&mut h, "engine.execute", json!({"command": "page.attach", "params": {"path": page, "source": dropped.to_string_lossy()}})).expect("attach");
    ask(&mut h, "ui.open", json!({"path": page})).expect("open");
    settle(&mut h);
    h.state_mut().set_show_source(true);
    settle(&mut h);

    let row = h.query_all_by_label_contains("assets/picture.png").next().expect("the file is listed");
    row.click();
    settle(&mut h);
    h.get_by_label("Delete file").click();
    settle(&mut h);

    // It asks first, and says plainly that undo will not bring a file back.
    assert!(h.query_all_by_label_contains("Undo does not bring a file back").count() >= 1, "the warning is there");
    h.get_by_label("Delete").click();
    settle(&mut h);

    assert_eq!(h.query_all_by_label_contains("assets/picture.png").count(), 0, "the file is off the page's list");
    let state = ask(&mut h, "ui.inspect", json!({})).expect("inspect");
    assert!(state["status"]["text"].as_str().unwrap_or_default().contains("gone"), "{}", state["status"]);
}

/// The folder decides what ink a page has: a drawing beside the page file is named by the
/// page even when the line that named it was lost, and a line naming ink that has gone is
/// dropped.
#[test]
fn a_page_names_the_ink_in_its_folder_and_no_ink_that_is_gone() {
    let (_t, app, s) = sample_app("ui-ink-names");
    let mut h = harness(app, LIGHT);
    h.run();
    let ideas = s.md_page.parent().expect("section").to_string_lossy().into_owned();
    let made = ask(
        &mut h,
        "engine.execute",
        json!({"command": "node.create", "params": {"parent": ideas, "kind": "page", "title": "Drawn", "format": "cmgfm"}}),
    )
    .expect("page");
    let page = made.as_str().expect("a path").to_string();
    ask(
        &mut h,
        "engine.execute",
        json!({"command": "page.save", "params": {"path": page, "body": "words only
"}}),
    )
    .expect("save");

    // A drawing in the folder that the page says nothing about: the state a deleted box
    // used to leave behind.
    let folder = s.md_page.parent().expect("section").join("drawn");
    let ink = "<?xml version=\"1.0\"?>
<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:nc=\"https://notecraft.org/ns/ink\" width=\"820\" height=\"1100\" viewBox=\"0 0 820 1100\">
<title>Ink</title>
<path nc:tool=\"pen\" nc:width=\"3\" nc:dabs=\"10,10,0.5 40,40,0.5\" d=\"M10 10 L40 40 Z\" fill=\"#202020\"/>
</svg>
";
    std::fs::write(folder.join("ink.svg"), ink).expect("a drawing beside the page");

    ask(&mut h, "ui.open", json!({"path": page})).expect("open");
    settle(&mut h);
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(
        body.contains("![Ink](ink.svg)"),
        "the page adopted the drawing in its folder:
{body}"
    );
    h.state_mut().save();
    let text = std::fs::read_to_string(folder.join("index.md")).expect("read the page file");
    assert!(
        text.contains("![Ink](ink.svg)"),
        "and the file on disk says so too:
{text}"
    );

    // Now the drawing goes, with the page still naming it.
    std::fs::remove_file(folder.join("ink.svg")).expect("remove the drawing");
    ask(&mut h, "ui.open", json!({"path": ideas})).ok();
    ask(&mut h, "ui.open", json!({"path": page})).expect("open again");
    settle(&mut h);
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(
        !body.contains("ink.svg"),
        "the page stopped naming ink that is not there:
{body}"
    );
}

/// Open paper takes a click and starts a new empty box where the pointer is, as it does in OneNote:
/// under the last words of a short page, and also beside or between boxes that are already there,
/// which only the strip under the last box did.
#[test]
fn a_click_on_open_paper_starts_a_new_box_where_it_was_made() {
    let (_t, app, s) = sample_app("ui-click-paper");
    let mut h = harness(app, LIGHT);
    h.run();
    let section = s.md_page.parent().expect("the section").to_string_lossy().into_owned();

    // A short page: clicks well under the last words, high and low in the window.
    let made = ask(
        &mut h,
        "engine.execute",
        json!({"command": "node.create", "params": {"parent": section, "kind": "page", "title": "Short", "format": "cmgfm"}}),
    )
    .expect("a page");
    let dir = made.as_str().expect("its path").to_string();
    ask(&mut h, "engine.execute", json!({"command": "page.save", "params": {"path": dir, "body": "# Short\n\nA few words.\n"}})).expect("save it");
    ask(&mut h, "ui.open", json!({"path": dir})).expect("open it");
    settle(&mut h);
    let page = h.state().page_rect().expect("the page");
    for y in [450.0, 600.0] {
        ask(&mut h, "ui.click", json!({"x": page.min.x + 200.0, "y": y})).expect("click the open paper");
        settle(&mut h);
        let state = ask(&mut h, "ui.inspect", json!({})).expect("inspect")["page"].clone();
        assert!(state["caret"] != Value::Null, "a click at y={y} under the words put the caret in a block: {state}");
        ask(&mut h, "ui.key", json!({"key": "Escape"})).expect("leave");
        settle(&mut h);
    }

    // An arranged page: two boxes side by side, and open paper to the right of them, at their height.
    let html = "<div class=\"nc-box\" data-x=\"0\" data-y=\"0\" data-width=\"260\"><p>left box</p></div>\n<div class=\"nc-box\" data-x=\"280\" data-y=\"0\" data-width=\"200\"><p>right box</p></div>\n";
    let made = ask(
        &mut h,
        "engine.execute",
        json!({"command": "node.create", "params": {"parent": section, "kind": "page", "title": "Boxes", "format": "html"}}),
    )
    .expect("a page");
    let dir = made.as_str().expect("its path").to_string();
    ask(&mut h, "engine.execute", json!({"command": "page.save", "params": {"path": dir, "body": html}})).expect("save it");
    ask(&mut h, "ui.open", json!({"path": dir})).expect("open it");
    settle(&mut h);
    let before = ask(&mut h, "ui.inspect", json!({})).expect("inspect")["page"].clone();
    let boxes_before = before["containers"].as_array().map_or(0, Vec::len);
    assert_eq!(boxes_before, 2, "two boxes to begin with: {before}");
    let origin = &before["origin"];
    let (ox, oy) = (origin[0].as_f64().unwrap_or(0.0), origin[1].as_f64().unwrap_or(0.0));
    // To the right of both boxes (they end at 480 across), at the height of the words.
    ask(&mut h, "ui.click", json!({"x": ox + 600.0, "y": oy + 24.0})).expect("click the open paper beside the boxes");
    settle(&mut h);
    let after = ask(&mut h, "ui.inspect", json!({})).expect("inspect")["page"].clone();
    assert_eq!(after["containers"].as_array().map_or(0, Vec::len), 3, "a click beside the boxes started a third: {after}");
    assert!(after["caret"] != Value::Null, "and the caret is in it: {after}");
    // Where it was made: its left edge is near the click, not at the end of the page.
    let third = &after["containers"][2];
    assert!((third["x"].as_f64().unwrap_or(0.0) - 594.0).abs() < 12.0, "at the pointer, across: {third}");
    assert!(third["y"].as_f64().unwrap_or(999.0) < 60.0, "at the pointer, down: {third}");

    // A click on a box's own words is still a click in that box, and starts nothing.
    ask(&mut h, "ui.key", json!({"key": "Escape"})).expect("leave");
    settle(&mut h);
    ask(&mut h, "ui.open", json!({"path": dir})).expect("open it again");
    settle(&mut h);
    let boxes_now = ask(&mut h, "ui.inspect", json!({})).expect("inspect")["page"]["containers"].as_array().map_or(0, Vec::len);
    ask(&mut h, "ui.click", json!({"x": ox + 60.0, "y": oy + 24.0})).expect("click in the left box");
    settle(&mut h);
    let inside = ask(&mut h, "ui.inspect", json!({})).expect("inspect")["page"].clone();
    assert_eq!(inside["containers"].as_array().map_or(0, Vec::len), boxes_now, "no new box for a click on a box: {inside}");
}

/// A box copied with Ctrl+C and pasted into another comes back as it was: its heading a heading, its link a
/// link, its diagram a diagram. The clipboard holds only Markdown text, and pasting that text as one
/// paragraph, as the ribbon's Paste once did, made a page of lines with their marks showing, blank lines
/// inside a paragraph and stray `<code>` pieces.
#[test]
fn a_box_that_is_copied_and_pasted_comes_back_as_it_was() {
    let board = MemoryBoard::new();
    let (_t, app, s) = sample_app("ui-copy-paste");
    let app = app.with_clipboard(Box::new(board.clone()));
    let mut h = harness(app, LIGHT);
    h.run();
    let section = s.md_page.parent().expect("the section").to_string_lossy().into_owned();
    let html = "<div class=\"nc-box\" data-x=\"0\" data-y=\"0\" data-width=\"400\">\n<h1>Copy me</h1>\n<p><a href=\"https://example.org\">a link</a></p>\n<pre class=\"diagram\">graph TD;\n  A--&gt;B;</pre>\n</div>\n";
    let made = ask(
        &mut h,
        "engine.execute",
        json!({"command": "node.create", "params": {"parent": section, "kind": "page", "title": "Copy", "format": "html"}}),
    )
    .expect("a page");
    let dir = made.as_str().expect("its path").to_string();
    ask(&mut h, "engine.execute", json!({"command": "page.save", "params": {"path": dir, "body": html}})).expect("save it");
    ask(&mut h, "ui.open", json!({"path": dir})).expect("open it");
    settle(&mut h);
    let page = ask(&mut h, "ui.inspect", json!({})).expect("inspect")["page"].clone();
    let (ox, oy) = (page["origin"][0].as_f64().unwrap_or(0.0), page["origin"][1].as_f64().unwrap_or(0.0));

    // Select the box by its bar, and copy it with the keys.
    ask(&mut h, "ui.click", json!({"x": ox + 200.0, "y": oy + 6.0})).expect("select the box by its bar");
    settle(&mut h);
    let selected = ask(&mut h, "ui.inspect", json!({})).expect("inspect")["page"]["selectedContainer"].clone();
    assert_eq!(selected, json!(0), "the box is selected: {selected}");
    press_copy(&mut h, &board, "C");
    settle(&mut h);

    // What that put on the clipboard: the Markdown of the box's blocks.
    let body = h.state().body().unwrap_or_default().to_string();
    let (blocks, _) = notecraft_formats::boxes::read(notecraft_model::PageFormat::Html, &body);
    let markdown = notecraft_formats::render::blocks_to_markdown(&blocks);
    assert!(markdown.contains("# Copy me") && markdown.contains("mermaid"), "the box as Markdown: {markdown}");

    // A new box on the open paper to the right, and paste into it.
    ask(&mut h, "ui.click", json!({"x": ox + 600.0, "y": oy + 120.0})).expect("start a box on the open paper");
    settle(&mut h);
    let started = ask(&mut h, "ui.inspect", json!({})).expect("inspect")["page"].clone();
    assert!(started["caret"] != Value::Null, "the new box has the caret: {started}");
    ask(&mut h, "ui.paste", json!({"text": markdown})).expect("paste");
    settle(&mut h);

    let after = h.state().body().unwrap_or_default().to_string();
    assert_eq!(after.matches("<h1>Copy me</h1>").count(), 2, "the heading came back as a heading:\n{after}");
    assert_eq!(after.matches("class=\"diagram\"").count(), 2, "and the diagram as a diagram:\n{after}");
    assert_eq!(after.matches("href=\"https://example.org\"").count(), 2, "and the link as a link:\n{after}");
    assert!(!after.contains("# Copy me"), "no Markdown marks showing as words:\n{after}");
    assert!(!after.contains("<code>"), "and no stray code pieces:\n{after}");
    assert!(!after.contains("<p>\n\n") && !after.contains("\n\n\n"), "no blank lines inside a paragraph:\n{after}");
}

/// What the page says of its own diagrams, once the threads that draw them have had their say.
fn drawn_in_page(h: &mut Harness<'_, NoteCraftApp>) -> Option<Value> {
    let began = std::time::Instant::now();
    loop {
        for _ in 0..4 {
            pump(h);
        }
        let state = ask(h, "ui.inspect", json!({})).ok()?;
        let d = state["pageDiagrams"].clone();
        if d["drawing"].as_u64() == Some(0) {
            return Some(d);
        }
        if began.elapsed() > std::time::Duration::from_secs(20) {
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

/// Every kind of chart and diagram the renderer is expected to draw is drawn in the page, from either
/// format, with the characters written raw: `-->` and `->>` are not escaped in an HTML `pre`.
#[test]
fn every_chart_and_diagram_type_is_drawn_in_both_formats() {
    let charts: [(&str, &str); 10] = [
        ("pie", "pie title Pages by format\n  \"HTML\" : 62\n  \"CommonMark\" : 31\n  \"Other\" : 7"),
        (
            "xychart",
            "xychart-beta\n    title \"Pages per month\"\n    x-axis [jan, feb, mar, apr, may]\n    y-axis \"Pages\" 0 --> 100\n    bar [20, 45, 60, 80, 95]\n    line [20, 45, 60, 80, 95]",
        ),
        (
            "quadrant",
            "quadrantChart\n    title What to build next\n    x-axis Low effort --> High effort\n    y-axis Low value --> High value\n    quadrant-1 Do next\n    quadrant-2 Plan\n    quadrant-3 Skip\n    quadrant-4 Maybe\n    Charts: [0.8, 0.9]\n    Icons: [0.2, 0.3]",
        ),
        (
            "gantt",
            "gantt\n  title Release\n  dateFormat YYYY-MM-DD\n  section Build\n  Band :done, a1, 2026-10-12, 5d\n  Charts :active, a2, after a1, 14d",
        ),
        ("sankey", "sankey-beta\nPages,HTML,62\nPages,Markdown,31\nHTML,Fast,40"),
        ("sequence", "sequenceDiagram\n  U->>N: Type\n  N-->>U: Saved"),
        ("state", "stateDiagram-v2\n  [*] --> Clean\n  Clean --> Dirty: type"),
        ("mindmap", "mindmap\n  root((NoteCraft))\n    Pages\n    Ink"),
        ("timeline", "timeline\n  title History\n  2026 : Band : Charts"),
        ("journey", "journey\n  title A day\n  section Work\n    Write: 5: Me\n    Test: 3: Me"),
    ];
    let (_t, app, s) = sample_app("ui-all-charts");
    let mut h = sized_harness(app, LIGHT, egui::vec2(1400.0, 1000.0));
    h.run();
    let section = s.md_page.parent().expect("the section").to_string_lossy().into_owned();
    let md: String = charts.iter().map(|(name, src)| format!("## {name}\n\n```mermaid\n{src}\n```\n\n")).collect();
    let html: String = charts.iter().map(|(name, src)| format!("<h2>{name}</h2>\n<pre class=\"mermaid\">{src}</pre>\n")).collect();
    for (title, format, body) in [("ChartsMd", "cmgfm", md), ("ChartsHtml", "html", html)] {
        let made = ask(
            &mut h,
            "engine.execute",
            json!({"command": "node.create", "params": {"parent": section, "kind": "page", "title": title, "format": format}}),
        )
        .expect("a page");
        let dir = made.as_str().expect("its path").to_string();
        ask(&mut h, "engine.execute", json!({"command": "page.save", "params": {"path": dir, "body": body}})).expect("save it");
        ask(&mut h, "ui.open", json!({"path": dir})).expect("open it");
        settle(&mut h);
        let d = drawn_in_page(&mut h).expect("drawn within twenty seconds");
        assert_eq!(
            (d["found"].clone(), d["drawn"].clone(), d["failed"].clone()),
            (json!(charts.len()), json!(charts.len()), json!(0)),
            "{title}: all {} kinds are found and drawn: {d}",
            charts.len()
        );
    }
}

/// A diagram is drawn in the page itself, in either format, without the code view; one that cannot
/// be drawn shows its source and says why; and clicking a drawn diagram puts the caret in it, so
/// its source can be edited, with the drawing following what is typed.
#[test]
fn a_diagram_is_drawn_in_the_page_and_its_source_opens_when_it_is_clicked() {
    let (_t, app, s) = sample_app("ui-diagram-live");
    let mut h = sized_harness(app, LIGHT, egui::vec2(1400.0, 1000.0));
    h.run();
    let section = s.md_page.parent().expect("the section").to_string_lossy().into_owned();
    let md_body =
        "# Live\n\n```mermaid\nflowchart TD\n  A[Start] --> B[Keep it]\n```\n\n```mermaid\nflowchart TD\n  A[unclosed --> B(\n  C -->\n```\n"
            .to_string();
    let html_body = "<h1>Live</h1>\n<pre class=\"mermaid\">flowchart TD\n  A[Start] --&gt; B[Keep it]</pre>\n<div class=\"mermaid\">pie\n  \"a\" : 1\n  \"b\" : 2</div>\n".to_string();
    let mut md_dir = String::new();
    for (title, format, body, found, failed) in [("LiveMd", "cmgfm", md_body, 2, 1), ("LiveHtml", "html", html_body, 2, 0)] {
        let made = ask(
            &mut h,
            "engine.execute",
            json!({"command": "node.create", "params": {"parent": section, "kind": "page", "title": title, "format": format}}),
        )
        .expect("a page");
        let dir = made.as_str().expect("its path").to_string();
        if title == "LiveMd" {
            md_dir = dir.clone();
        }
        ask(&mut h, "engine.execute", json!({"command": "page.save", "params": {"path": dir, "body": body}})).expect("save it");
        ask(&mut h, "ui.open", json!({"path": dir})).expect("open it");
        settle(&mut h);
        assert!(!h.state().show_source(), "this is the live page, not the code view");
        let d = drawn_in_page(&mut h).expect("drawn within twenty seconds");
        assert_eq!(d["found"], json!(found), "{title}: {d}");
        assert_eq!(d["failed"], json!(failed), "{title}: {d}");
        assert_eq!(d["drawn"], json!(found - failed), "{title}: every diagram that can be drawn is: {d}");
        if failed > 0 {
            assert!(h.query_all_by_label_contains("Not drawn:").count() >= 1, "{title}: the one that failed says why, in the page");
            assert!(h.query_all_by_label_contains("unclosed").count() >= 1, "{title}: and shows its source");
        } else {
            assert!(h.query_all_by_label_contains("flowchart TD").count() == 0, "{title}: a drawn diagram does not also show its source");
        }
    }

    // The caret in a drawn diagram shows its source to edit; what is typed is drawn as it goes.
    ask(&mut h, "ui.open", json!({"path": md_dir})).expect("open the Markdown page again");
    settle(&mut h);
    let caret = ask(&mut h, "ui.caret", json!({"block": 1})).expect("put the caret in the first diagram");
    assert!(caret["text"].as_str().unwrap_or("").contains("flowchart TD"), "the caret is in the diagram's source: {caret}");
    for _ in 0..6 {
        pump(&mut h);
    }
    assert!(h.query_all_by_label_contains("flowchart TD").count() >= 1, "its source is shown to edit");
    let d = drawn_in_page(&mut h).expect("still drawn");
    assert_eq!((d["found"].clone(), d["drawn"].clone()), (json!(2), json!(1)), "the drawing stays while its source is edited: {d}");
    ask(&mut h, "ui.key", json!({"key": "End", "ctrl": true})).expect("to the end of the source");
    ask(
        &mut h,
        "ui.text",
        json!({"text": "
  B --> D[Typed word]"}),
    )
    .expect("type a line");
    for _ in 0..6 {
        pump(&mut h);
    }
    let d = drawn_in_page(&mut h).expect("drawn again");
    assert_eq!(
        (d["found"].clone(), d["drawn"].clone(), d["failed"].clone()),
        (json!(2), json!(1), json!(1)),
        "the edited diagram is drawn again: {d}"
    );
    settle(&mut h);
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(
        body.contains("Typed word"),
        "what was typed is in the page:
{body}"
    );
}

/// A diagram kept as a file of its own beside the page is listed in the code view as a file, shown as
/// its source, and what is typed in it is written to that file. The code view shows code, not the
/// drawing.
#[test]
fn a_diagram_file_beside_the_page_is_listed_and_its_source_can_be_edited() {
    let (_t, app, s) = sample_app("ui-diagram-file");
    let mut h = harness(app, LIGHT);
    h.run();
    let source = "flowchart LR\n  A[Source file] --> B[Drawing]\n";
    std::fs::write(s.md_page.join("flow.mmd"), source).expect("a diagram file");
    ask(&mut h, "ui.open", json!({"path": s.md_page.to_string_lossy()})).expect("open");
    settle(&mut h);
    h.state_mut().set_show_source(true);
    settle(&mut h);
    let row = h.query_all_by_label_contains("flow.mmd").next().expect("the diagram file is in the list").rect();
    ask(&mut h, "ui.click", json!({"x": row.center().x, "y": row.center().y})).expect("pick it");
    settle(&mut h);
    let state = ask(&mut h, "ui.inspect", json!({})).expect("inspect");
    assert_eq!(state["codeView"]["picked"], json!("flow.mmd"), "{}", state["codeView"]);
    assert_eq!(
        state["codeView"]["characters"].as_u64().unwrap_or(0) as usize,
        source.chars().count(),
        "its source is in hand: {}",
        state["codeView"]
    );
    assert!(h.query_all_by_label_contains("read-only").count() == 0, "it can be typed in");
    assert!(h.query_all_by_label_contains("Diagram").count() == 0, "the code view shows the code and no drawing");
}

/// A diagram too long to be drawn is refused at once, with its source kept in the page, and does not
/// set the renderer to work.
#[test]
fn a_diagram_past_the_size_limit_is_refused_at_once_and_its_source_is_kept() {
    let (_t, app, s) = sample_app("ui-diagram-limit");
    let mut h = harness(app, LIGHT);
    h.run();
    let big: String = std::iter::once("flowchart TD\n".to_string())
        .chain((1..=300).map(|i| format!("  N{i}[Node {i}] --> N{}[Node {}]\n", i + 1, i + 1)))
        .collect();
    let body = format!("# Big\n\n```mermaid\n{big}```\n");
    ask(&mut h, "engine.execute", json!({"command": "page.save", "params": {"path": s.md_page.to_string_lossy(), "body": body}})).expect("save it");
    ask(&mut h, "ui.open", json!({"path": s.md_page.to_string_lossy()})).expect("open");
    settle(&mut h);
    let began = std::time::Instant::now();
    let d = drawn_in_page(&mut h).expect("settled");
    assert!(began.elapsed() < std::time::Duration::from_secs(5), "refused at once, not after the renderer's two seconds: {:?}", began.elapsed());
    assert_eq!((d["found"].clone(), d["failed"].clone()), (json!(1), json!(1)), "{d}");
    assert!(h.query_all_by_label_contains("Not drawn:").count() >= 1, "the page says why");
    let text = std::fs::read_to_string(s.md_page.join("index.md")).expect("the page file");
    assert!(text.contains("N300[Node 300]"), "the source is untouched");
}

/// The code view names the language of the file it shows and colours it, and a Find bar under the
/// editor looks in that file: Ctrl+F opens it, typing finds, Enter and Shift+Enter step through the
/// matches, and Escape closes it with the match selected in the text.
#[test]
fn the_code_view_colours_the_file_and_finds_text_in_it() {
    let (_t, app, s) = sample_app("ui-code-find");
    let mut h = harness(app, LIGHT);
    h.run();
    ask(&mut h, "ui.open", json!({"path": s.md_page.to_string_lossy()})).expect("open");
    settle(&mut h);
    h.state_mut().set_show_source(true);
    settle(&mut h);
    let view = |h: &mut Harness<'_, NoteCraftApp>| ask(h, "ui.inspect", json!({})).expect("inspect")["codeView"].clone();
    let v = view(&mut h);
    assert_eq!(v["language"], json!("Markdown"), "{v}");
    assert!(v["colouredStretches"].as_u64().unwrap_or(0) > 10, "the file is coloured: {v}");
    assert_eq!(v["find"]["open"], json!(false), "no Find bar until it is asked for: {v}");

    ask(&mut h, "ui.key", json!({"key": "F", "ctrl": true})).expect("Ctrl+F");
    settle(&mut h);
    assert_eq!(view(&mut h)["find"]["open"], json!(true), "Ctrl+F opened the bar");
    assert!(h.query_all_by_label_contains("Match case").count() >= 1, "the bar is on the screen");

    // The word in the sample page that comes more than once.
    ask(&mut h, "ui.text", json!({"text": "page"})).expect("type the query");
    settle(&mut h);
    let f = view(&mut h)["find"].clone();
    assert_eq!(f["query"], json!("page"), "{f}");
    let count = f["matches"].as_u64().unwrap_or(0);
    assert!(count >= 2, "the word is found more than once: {f}");
    assert_eq!(f["current"], json!(1), "it starts on the first match: {f}");

    ask(&mut h, "ui.key", json!({"key": "Enter"})).expect("Enter");
    settle(&mut h);
    assert_eq!(view(&mut h)["find"]["current"], json!(2), "Enter goes to the next match");
    ask(&mut h, "ui.key", json!({"key": "Enter", "shift": true})).expect("Shift+Enter");
    settle(&mut h);
    assert_eq!(view(&mut h)["find"]["current"], json!(1), "Shift+Enter goes back");
    ask(&mut h, "ui.key", json!({"key": "Enter", "shift": true})).expect("Shift+Enter again");
    settle(&mut h);
    assert_eq!(view(&mut h)["find"]["current"], json!(count), "and from the first it wraps to the last");

    // Case: the capital P of "Page" is not the word unless the case is asked for.
    let before = view(&mut h)["find"]["matches"].as_u64().unwrap_or(0);
    let button = h.get_by_label("Match case").rect();
    ask(&mut h, "ui.click", json!({"x": button.center().x, "y": button.center().y})).expect("Match case");
    settle(&mut h);
    let f = view(&mut h)["find"].clone();
    assert_eq!(f["matchCase"], json!(true), "{f}");
    assert!(f["matches"].as_u64().unwrap_or(0) <= before, "matching case cannot find more: {f}");

    ask(&mut h, "ui.key", json!({"key": "Escape"})).expect("Escape");
    settle(&mut h);
    assert_eq!(view(&mut h)["find"]["open"], json!(false), "Escape closed the bar");
    assert!(h.query_all_by_label_contains("Match case").count() == 0, "and it is gone from the screen");

    // The page file is Markdown, its drawing is XML, and a file nobody has a name for is plain.
    std::fs::write(s.md_page.join("data.json"), "{\"a\": [1, true, null]}").expect("a json file");
    std::fs::write(s.md_page.join("notes.txt"), "plain words").expect("a text file");
    ask(&mut h, "ui.open", json!({"path": s.md_page.to_string_lossy()})).expect("open again");
    settle(&mut h);
    for (file, language) in [("data.json", "JSON"), ("notes.txt", "Plain text")] {
        let row = h.query_all_by_label_contains(file).next().expect("the file is listed").rect();
        ask(&mut h, "ui.click", json!({"x": row.center().x, "y": row.center().y})).expect("pick it");
        settle(&mut h);
        assert_eq!(view(&mut h)["language"], json!(language), "{file}");
    }
}

/// The code view stays open while another page is picked in the tree. It must then show that
/// page's file: the text of the last page, kept under the same name, would be shown as the new
/// page's own, and whatever was typed would be written over the new page.
#[test]
fn the_code_view_follows_the_page_it_is_open_on() {
    let (_t, app, s) = sample_app("ui-code-follows");
    let mut h = harness(app, LIGHT);
    h.run();
    ask(&mut h, "ui.open", json!({"path": s.md_page.to_string_lossy()})).expect("open the first page");
    settle(&mut h);
    h.state_mut().set_show_source(true);
    settle(&mut h);
    let first = std::fs::read_to_string(s.md_page.join("index.md")).expect("the first page's file");
    let state = ask(&mut h, "ui.inspect", json!({})).expect("inspect");
    assert_eq!(state["codeView"]["characters"].as_u64().unwrap_or(0) as usize, first.chars().count(), "{}", state["codeView"]);

    // A second page of the same format: its file has the same name, `index.md`, which is the case
    // where the name alone cannot tell the two pages apart.
    let section = s.md_page.parent().expect("the section").to_string_lossy().into_owned();
    let made = ask(
        &mut h,
        "engine.execute",
        json!({"command": "node.create", "params": {"parent": section, "kind": "page", "title": "Second", "format": "cmgfm"}}),
    )
    .expect("a second page");
    let second_dir = made.as_str().expect("its path").to_string();
    ask(
        &mut h,
        "engine.execute",
        json!({"command": "page.save", "params": {"path": second_dir, "body": "# Second

A page of its own, much shorter than the first.
"}}),
    )
    .expect("save it");
    ask(&mut h, "ui.open", json!({"path": second_dir})).expect("open the second page with the view open");
    settle(&mut h);
    let second = std::fs::read_to_string(std::path::Path::new(&second_dir).join("index.md")).expect("the second page's file");
    assert_ne!(first.chars().count(), second.chars().count(), "the two pages differ, or this test proves nothing");
    let state = ask(&mut h, "ui.inspect", json!({})).expect("inspect");
    assert_eq!(state["codeView"]["picked"], json!("index.md"), "{}", state["codeView"]);
    assert_eq!(
        state["codeView"]["characters"].as_u64().unwrap_or(0) as usize,
        second.chars().count(),
        "the second page's own file is in hand, not the first's: {}",
        state["codeView"]
    );
}

/// The code view: the page's files down the left, the one that is picked on the right.
/// The page file's body can be typed in, a drawing's SVG can be typed in, and a picture
/// nothing names is marked as not used.
#[test]
fn the_code_view_lists_the_pages_files_and_shows_the_one_that_is_picked() {
    let (_t, app, s) = sample_app("ui-code-view");
    let mut h = harness(app, LIGHT);
    h.run();
    let page = s.md_page.to_string_lossy().into_owned();
    let ink = "<?xml version=\"1.0\"?>
<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:nc=\"https://notecraft.org/ns/ink\" width=\"820\" height=\"1100\">
<title>Ink</title>
<path nc:tool=\"pen\" nc:width=\"3\" nc:dabs=\"10,10,0.5 40,40,0.5\" d=\"M10 10 L40 40 Z\" fill=\"#202020\"/>
</svg>
";
    std::fs::write(s.md_page.join("ink.svg"), ink).expect("a drawing beside the page");
    std::fs::write(s.md_page.join("stray.txt"), "nothing on the page names this").expect("an orphan");
    ask(&mut h, "ui.open", json!({"path": page})).expect("open");
    settle(&mut h);

    h.state_mut().set_show_source(true);
    settle(&mut h);

    // The list: the page file first, then what sits beside it, then what is in assets.
    for row in ["index.md", "ink.svg", "assets/"] {
        assert!(h.query_all_by_label_contains(row).count() >= 1, "{row} is in the list");
    }
    // The page file is picked when the view opens, and the whole file is in hand: the
    // head as well as the words, so a line in the head can be put right here.
    // Picked again after everything has settled, so the text in hand is the file as it
    // now stands: the page adopts the drawing in its folder when it opens, which writes
    // the file once.
    let stray_row = h.query_all_by_label_contains("stray.txt").next().expect("a row").rect();
    ask(&mut h, "ui.click", json!({"x": stray_row.center().x, "y": stray_row.center().y})).expect("another file");
    settle(&mut h);
    let page_row = h.query_all_by_label_contains("index.md").next().expect("the page file's row").rect();
    ask(&mut h, "ui.click", json!({"x": page_row.center().x, "y": page_row.center().y})).expect("the page file");
    settle(&mut h);
    let state = ask(&mut h, "ui.inspect", json!({})).expect("inspect");
    assert_eq!(state["codeView"]["picked"], json!("index.md"), "{}", state["codeView"]);
    let whole = std::fs::read_to_string(s.md_page.join("index.md")).expect("read the page file");
    assert_eq!(
        state["codeView"]["characters"].as_u64().unwrap_or(0) as usize,
        whole.chars().count(),
        "the whole file, head and all: {}",
        state["codeView"]
    );
    review(&mut h, "code_view");

    // Picking the drawing shows its SVG, which can be typed in.
    let row = h.query_all_by_label_contains("ink.svg").next().expect("the drawing's row").rect();
    ask(&mut h, "ui.click", json!({"x": row.center().x, "y": row.center().y})).expect("pick it");
    settle(&mut h);
    let state = ask(&mut h, "ui.inspect", json!({})).expect("inspect");
    assert_eq!(state["codeView"]["picked"], json!("ink.svg"), "the drawing is picked: {}", state["codeView"]);
    // A drawing is drawn, not shown as a wall of numbers, and says how many strokes it
    // holds; the code is one click away.
    assert!(h.query_all_by_label_contains("1 stroke").count() >= 1, "it says what is in the drawing");
    h.get_by_label("Show the code").click();
    settle(&mut h);
    let state = ask(&mut h, "ui.inspect", json!({})).expect("inspect");
    assert!(state["codeView"]["characters"].as_u64().unwrap_or(0) > 100, "its SVG is in hand: {}", state["codeView"]);
    assert!(h.query_all_by_label_contains("The pen's own readings").count() >= 1, "with a word about what is in it");
    h.get_by_label("Show the strokes").click();
    settle(&mut h);
    assert!(h.query_all_by_label_contains("Show the code").count() >= 1, "and back to the drawing");
    review(&mut h, "code_view_ink");

    // A file nothing names is marked, so an orphan is not mistaken for one in use, and
    // it cannot be typed in.
    let stray = h.query_all_by_label_contains("stray.txt").next().expect("the stray file's row").rect();
    ask(&mut h, "ui.click", json!({"x": stray.center().x, "y": stray.center().y})).expect("pick it");
    settle(&mut h);
    assert!(h.query_all_by_label_contains("not used by the page").count() >= 1, "the file nothing names is marked");
    assert!(h.query_all_by_label_contains("read-only").count() >= 1, "and it cannot be typed in");

    // Closing the view puts the page back.
    h.state_mut().set_show_source(false);
    settle(&mut h);
    assert_eq!(h.query_all_by_label_contains("FILES IN THIS PAGE").count(), 0, "the list is gone with the view");
}

/// The paper a page is drawn on and the colour behind it are the page's own: chosen in
/// Format Background, written into the page's file, and there again when it is reopened.
#[test]
fn a_page_keeps_the_paper_and_the_colour_it_was_given() {
    let (_t, app, s) = sample_app("ui-page-paper");
    let mut h = harness(app, LIGHT);
    h.run();
    let page = s.md_page.to_string_lossy().into_owned();
    ask(&mut h, "ui.open", json!({"path": page})).expect("open");
    settle(&mut h);
    let before = ask(&mut h, "ui.inspect", json!({})).expect("inspect");
    assert_eq!(before["paper"], json!("none"), "a page says nothing about paper to begin with");
    assert_eq!(before["pageColour"], json!(null));

    // Draw, then Format Background: the gallery of papers and the page colours.
    h.get_by_label("Draw").click();
    settle(&mut h);
    h.get_by_label("Format Background").click();
    settle(&mut h);
    h.get_by_label("College ruled").click();
    settle(&mut h);
    h.get_by_label("Format Background").click();
    settle(&mut h);
    h.get_by_label("Yellow").click();
    settle(&mut h);

    let now = ask(&mut h, "ui.inspect", json!({})).expect("inspect");
    assert_eq!(now["paper"], json!("college"), "the page is on college ruled paper: {now}");
    assert_eq!(now["pageColour"], json!("yellow"), "and it is yellow: {now}");

    // Written into the page's own file, and read back from it.
    let text = std::fs::read_to_string(s.md_page.join("index.md")).expect("read");
    assert!(
        text.contains("paper: college"),
        "the page's file says so:
{text}"
    );
    assert!(
        text.contains("page-color: yellow"),
        "and names the colour:
{text}"
    );

    ask(&mut h, "ui.open", json!({"path": s.html_page.to_string_lossy()})).expect("another page");
    settle(&mut h);
    let other = ask(&mut h, "ui.inspect", json!({})).expect("inspect");
    assert_eq!(other["pageColour"], json!(null), "another page is not coloured by it: {other}");

    ask(&mut h, "ui.open", json!({"path": page})).expect("back");
    settle(&mut h);
    let again = ask(&mut h, "ui.inspect", json!({})).expect("inspect");
    assert_eq!(again["paper"], json!("college"), "and the page still has its own paper");
    assert_eq!(again["pageColour"], json!("yellow"));
    review(&mut h, "page_paper");
}

/// Copy link to page, the way OneNote copies one: the link names the page from the
/// notebooks folder down, so it works wherever it is pasted, and clicking it opens the
/// page.
#[test]
fn a_link_to_a_page_can_be_copied_and_pasted_into_another_page() {
    let board = MemoryBoard::new();
    let (_t, app, s) = sample_app("ui-copy-link");
    let app = app.with_clipboard(Box::new(board.clone()));
    let mut h = harness(app, LIGHT);
    h.run();

    // The link to the Markdown page, taken the way the tree's menu takes it.
    h.state_mut().copy_link_to(&s.md_page);
    for _ in 0..8 {
        pump(&mut h);
        forward_copies(&mut h, &board);
    }
    let status = ask(&mut h, "ui.inspect", json!({})).expect("inspect copied link");
    assert_eq!(status.pointer("/status/text").and_then(Value::as_str), Some("Link to \"Format ideas\" copied"));

    // Pasted into another page, it is a link, and clicking it opens the page it names.
    let other = s.html_page.to_string_lossy().into_owned();
    ask(&mut h, "ui.open", json!({"path": other})).expect("open the other page");
    settle(&mut h);
    let root = s.notebook.parent().map(PathBuf::from).unwrap_or_else(|| s.notebook.clone());
    let uri = notecraft_store::links::page_uri(&[root], &s.md_page).unwrap_or_default();
    ask(&mut h, "engine.execute", json!({"command": "page.save", "params": {"path": other, "body": format!("<p><a href=\"{uri}\">go</a></p>")}}))
        .expect("save with the link");
    ask(&mut h, "ui.open", json!({"path": other})).expect("open again");
    settle(&mut h);
    let link = h.get_by_label("go").rect();
    ask(&mut h, "ui.click", json!({"x": link.center().x, "y": link.center().y})).expect("click the link");
    settle(&mut h);
    let state = ask(&mut h, "ui.inspect", json!({})).expect("inspect");
    assert_eq!(state["page"]["title"], json!("Format ideas"), "the link opened the page it names: {}", state["page"]);
}

/// A shape from the Draw tab is drawn by dragging on the page, and what it leaves is ink:
/// a stroke in the page's own drawing, which the eraser and Select can act on.
#[test]
fn a_shape_is_drawn_on_the_page_as_ink() {
    let (_t, app, s) = sample_app("ui-shapes");
    // The Draw tab has more groups than a narrow window shows, and Shapes is at the far
    // end of it: a button that is scrolled off cannot be clicked.
    let mut h = sized_harness(app, LIGHT, egui::vec2(2000.0, 900.0));
    h.run();
    let page = s.md_page.to_string_lossy().into_owned();
    ask(&mut h, "ui.open", json!({"path": page})).expect("open");
    settle(&mut h);

    h.get_by_label("Draw").click();
    settle(&mut h);
    h.get_by_label("Rectangle").click();
    settle(&mut h);
    let state = ask(&mut h, "ui.inspect", json!({})).expect("inspect");
    assert_eq!(state["ink"]["pointer"], json!("rectangle"), "the rectangle is in hand: {}", state["ink"]);

    // Drag out the two corners it is drawn between.
    ask(&mut h, "ui.press", json!({"x": 620, "y": 420})).expect("press");
    settle(&mut h);
    ask(&mut h, "ui.move", json!({"x": 760, "y": 520})).expect("drag");
    settle(&mut h);
    ask(&mut h, "ui.release", json!({"x": 760, "y": 520})).expect("let go");
    settle(&mut h);

    // It is in the page's drawing, and in the file beside the page.
    let state = ask(&mut h, "ui.inspect", json!({})).expect("inspect");
    assert_eq!(state["ink"]["inked"], json!(true), "the page has ink on it now: {}", state["ink"]);
    let svg = std::fs::read_to_string(s.md_page.join("ink.svg")).expect("the drawing was written");
    assert!(svg.contains("nc:dabs"), "with the pen's own readings in it");
    assert_eq!(
        svg.matches("<path").count(),
        1,
        "one stroke, the shape:
{svg:.400}"
    );
    review(&mut h, "shape_drawn");

    // Undo Stroke takes it back, as it does any other stroke.
    h.get_by_label("Undo stroke").click();
    settle(&mut h);
    let state = ask(&mut h, "ui.inspect", json!({})).expect("inspect");
    assert_eq!(state["ink"]["inked"], json!(false), "the shape was taken back: {}", state["ink"]);
}

/// A table handed to a spreadsheet and read back again: what the spreadsheet holds is
/// what the page's table says afterwards.
#[test]
fn a_table_is_read_back_from_the_spreadsheet_beside_the_page() {
    let (_t, app, s) = sample_app("ui-grid-back");
    // Reading back needs no program, only the spreadsheet's file, but the button is offered only
    // where GridCraft is installed, so the test says it is: a runner that has none must pass too.
    let app = app.with_gridcraft(Some(std::path::PathBuf::from("gridcraft")));
    let mut h = sized_harness(app, LIGHT, egui::vec2(2000.0, 900.0));
    h.run();
    let ideas = s.md_page.parent().expect("section").to_string_lossy().into_owned();
    let made = ask(
        &mut h,
        "engine.execute",
        json!({"command": "node.create", "params": {"parent": ideas, "kind": "page", "title": "Budget", "format": "cmgfm"}}),
    )
    .expect("page");
    let page = made.as_str().expect("a path").to_string();
    ask(
        &mut h,
        "engine.execute",
        json!({"command": "page.save", "params": {"path": page, "body": "| Item | Cost |
|---|---|
| Tea | 3 |
"}}),
    )
    .expect("save");
    ask(&mut h, "ui.open", json!({"path": page})).expect("open");
    settle(&mut h);

    // The spreadsheet's own work, as GridCraft would leave it beside the page.
    let folder = s.md_page.parent().expect("section").join("budget");
    std::fs::write(
        folder.join("table-1.csv"),
        "Item,Cost,Note
\"Tea, green\",3.50,\"she said \"\"best\"\"\"
Paper,12,
",
    )
    .expect("the csv");

    let cell = h.get_by_label("Tea").rect();
    ask(&mut h, "ui.click", json!({"x": cell.center().x, "y": cell.center().y})).expect("into the table");
    settle(&mut h);
    h.get_by_label("Table").click();
    settle(&mut h);
    h.get_by_label("Read back from it").click();
    settle(&mut h);

    let body = h.state().body().unwrap_or_default().to_string();
    assert!(
        body.contains("| Note |"),
        "the column the spreadsheet added is there:
{body}"
    );
    assert!(
        body.contains("Tea, green"),
        "and a cell with a comma in it came back whole:
{body}"
    );
    assert!(
        body.contains("she said \"best\""),
        "with its quotes undone:
{body}"
    );
    let state = ask(&mut h, "ui.inspect", json!({})).expect("inspect");
    assert!(state["status"]["text"].as_str().unwrap_or_default().contains("3 rows"), "{}", state["status"]);
}

/// A tab with many groups in a narrow window: the Draw tab has eleven, as OneNote's does,
/// and they are wider than a small window. They scroll rather than being cut off, and every
/// one of them is still there to be reached.
#[test]
fn the_ribbon_scrolls_rather_than_cutting_a_group_off() {
    let (_t, app, _s) = sample_app("ui-ribbon-narrow");
    let mut h = harness(app, LIGHT);
    h.run();
    h.get_by_label("Draw").click();
    h.run();
    let groups = ["Undo", "Selection", "Drawing Tools", "Input Mode", "Stencils", "Edit", "Shapes", "Text", "Math", "Replay", "Mode", "Help"];

    for width in [1400.0, 1100.0, 820.0, 640.0] {
        ask(&mut h, "ui.resize", json!({"width": width, "height": 700.0})).expect("resize");
        settle(&mut h);
        for group in groups {
            assert!(h.query_all_by_label(group).count() >= 1, "{group} is still there at {width} wide");
        }
        // Nothing drew itself off the left edge, which is what clipping looked like.
        for group in groups {
            // A name can belong to a group and to a button inside it; every one of them
            // has to be inside the window, so they are all checked.
            for node in h.get_all_by_label(group) {
                let rect = node.rect();
                assert!(rect.min.x >= -1.0, "{group} starts inside the window at {width} wide: {rect:?}");
            }
        }
    }
    ask(&mut h, "ui.resize", json!({"width": 820.0, "height": 700.0})).expect("narrow");
    settle(&mut h);
    review(&mut h, "ribbon_narrow");
}

/// Rule Lines opens a gallery, not a list of words: every paper is drawn small so the look
/// is picked by looking. Each thumbnail is still a button with the paper's name, which is
/// how a screen reader reads it and how this test picks one.
#[test]
fn the_rule_lines_gallery_shows_every_paper_and_picking_one_rules_the_page() {
    let (_t, mut app, s) = sample_app("ui-rule-gallery");
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    h.get_by_label("View").click();
    h.run();
    h.get_by_label("Format Background").click();
    h.run();

    // All eleven papers are on the screen, each as its own thumbnail.
    for paper in [
        "None",
        "Narrow ruled",
        "College ruled",
        "Standard ruled",
        "Wide ruled",
        "Small squares",
        "Medium squares",
        "Large squares",
        "Very large squares",
        "NoteCraft ruled",
        "NoteCraft squares",
    ] {
        assert!(h.query_all_by_label(paper).count() >= 1, "{paper} is in the gallery");
    }
    review(&mut h, "rule_lines_gallery");

    // Picking one rules the page and closes the gallery, and the button says what is on.
    h.get_by_label("NoteCraft squares").click();
    h.run();
    h.run();
    let paper = ask(&mut h, "ui.inspect", json!({})).expect("inspect")["paper"].as_str().unwrap_or_default().to_string();
    assert_eq!(paper, "grid", "the page is on NoteCraft's squared paper");
    assert_eq!(h.query_all_by_label("Wide ruled").count(), 0, "the gallery closed");
}

#[test]
fn following_the_system_keeps_the_accent() {
    let t = TempDir::new("ui-system");
    let mut app = NoteCraftApp::new(session(&t));
    app.follow_system_theme(Accent::Green);
    assert_eq!(app.mode_pref(), ModePref::System);
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 760.0)).build_ui_state(|ui, a: &mut NoteCraftApp| a.show(ui), app);
    h.run();
    assert_eq!(h.state().theme().accent, Accent::Green);
}

/// Renders the window in each of the 14 themes and checks the pixels: the page is not blank,
/// the main area has the theme's panel colour, and the accent colour appears.
#[test]
fn all_fourteen_themes_render_with_their_own_colours() {
    for theme in Theme::all() {
        let (_t, mut app, s) = sample_app("ui-all");
        app.open_path(&s.md_page.to_string_lossy());
        let mut h = harness(app, theme);
        settle(&mut h);
        let img = h.render().expect("render");
        let tokens = theme.tokens();
        let near = |p: &image::Rgba<u8>, c: egui::Color32| {
            (i32::from(p.0[0]) - i32::from(c.r())).abs() <= 3
                && (i32::from(p.0[1]) - i32::from(c.g())).abs() <= 3
                && (i32::from(p.0[2]) - i32::from(c.b())).abs() <= 3
        };
        let panel_pixels = img.pixels().filter(|p| near(p, tokens.panel)).count();
        let total = img.pixels().count();
        assert!(panel_pixels * 5 > total, "{}: panel colour covers only {panel_pixels} of {total} pixels", theme.label());
        assert!(img.pixels().any(|p| near(p, tokens.accent)), "{}: the accent colour is missing", theme.label());
        let colours: std::collections::HashSet<[u8; 4]> = img.pixels().map(|p| p.0).collect();
        assert!(colours.len() > 60, "{}: the picture looks blank", theme.label());
        assert!(contrast(tokens.text, tokens.panel) >= 7.0);
        review(&mut h, &format!("theme_{}", theme.id()));
    }
}

#[test]
fn rows_and_swatches_are_reachable_by_accessibility_labels() {
    let (_t, mut app, _s) = sample_app("ui-a11y");
    app.expand_all();
    let mut h = harness(app, LIGHT);
    h.run();
    for label in ["Work", "Projects", "Project X", "Level 6", "Ideas", "Format ideas"] {
        let _ = h.get_by_label(label);
    }
}

#[test]
fn results_of_an_unused_output_variant_compile() {
    // Keeps the import of `Output` honest: the engine's result type is part of the public surface the UI uses.
    let _ = Output::None;
    let _ = params(&[("a", "b")]);
}

/// Two containers on the sample page, and where the first one is drawn: its top left corner.
fn two_boxes(label: &str) -> (TempDir, Harness<'static, NoteCraftApp>, u64, (f64, f64)) {
    two_boxes_on(label, &MemoryBoard::new())
}

/// [`two_boxes`], with the app's clipboard being `board`.
fn two_boxes_on(label: &str, board: &MemoryBoard) -> (TempDir, Harness<'static, NoteCraftApp>, u64, (f64, f64)) {
    let (t, app, s) = sample_app(label);
    let mut app = app.with_clipboard(Box::new(board.clone()));
    let page = s.md_page.to_string_lossy().to_string();
    app.open_path(&page);
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    let blocks = ask(&mut h, "ui.inspect", json!({})).expect("inspect").pointer("/page/blocks").and_then(Value::as_u64).unwrap_or(0);
    assert!(blocks >= 4, "the sample page has blocks in it: {blocks}");
    let line = format!("0,0,320,2; 360,30,300,{}", blocks - 2);
    ask(&mut h, "engine.execute", json!({"command": "page.containers", "params": {"path": page.clone(), "boxes": line}})).expect("arrange");
    ask(&mut h, "ui.open", json!({"path": page})).expect("open");
    settle(&mut h);
    let v = ask(&mut h, "ui.inspect", json!({})).expect("inspect");
    let origin = v.pointer("/page/origin").and_then(Value::as_array).cloned().unwrap_or_default();
    let at = (origin.first().and_then(Value::as_f64).unwrap_or(0.0), origin.get(1).and_then(Value::as_f64).unwrap_or(0.0));
    assert!(at.0 > 0.0 && at.1 > 0.0, "the page knows where it is drawn: {origin:?}");
    (t, h, blocks, at)
}

fn look_at(h: &mut Harness<'_, NoteCraftApp>) -> Value {
    ask(h, "ui.inspect", json!({})).expect("inspect")
}

/// A right click on a container selects it, with everything in it, and its menu's Delete takes
/// the box and its content off the page.
#[test]
fn right_click_selects_a_container_and_delete_removes_it_with_its_content() {
    let (_t, mut h, blocks, (ox, oy)) = two_boxes("ui-box-select");
    let before = h.state().body().unwrap_or_default().to_string();

    // Inside the first box, below its handle, on its words.
    ask(&mut h, "ui.click", json!({"x": ox + 100.0, "y": oy + 30.0, "button": "right"})).expect("right click");
    for _ in 0..6 {
        pump(&mut h);
    }
    let v = look_at(&mut h);
    assert_eq!(v.pointer("/page/selectedContainer").and_then(Value::as_u64), Some(0), "the box under the pointer is selected: {v}");
    assert!(v.pointer("/page/caret").is_none_or(Value::is_null), "no caret: the whole box is selected, not a word in it");
    assert!(h.query_all_by_label("Delete").count() >= 1, "the box has its own menu with Delete on it");
    assert!(h.query_all_by_label("Table").count() >= 1, "and the page's own entries are on it too, so none were lost");

    h.query_all_by_label("Delete").last().expect("a Delete item").click();
    settle(&mut h);
    let v = look_at(&mut h);
    let boxes = v.pointer("/page/containers").and_then(Value::as_array).cloned().unwrap_or_default();
    assert_eq!(boxes.len(), 1, "one box is left: {boxes:?}");
    assert_eq!(v.pointer("/page/blocks").and_then(Value::as_u64), Some(blocks - 2), "the box's two blocks went with it");
    assert_eq!(boxes.first().and_then(|b| b.get("blocks")).and_then(Value::as_u64), Some(blocks - 2), "and the other box holds what is left");
    assert!(v.pointer("/page/selectedContainer").is_none_or(Value::is_null), "nothing is selected any more");
    let after = h.state().body().unwrap_or_default().to_string();
    assert!(after.len() < before.len(), "the page is shorter: {} then {}", before.len(), after.len());
}

/// A click on the handle selects too, and the Delete key then removes the box and its content.
#[test]
fn the_handle_selects_a_container_and_the_delete_key_removes_it() {
    let (_t, mut h, blocks, (ox, oy)) = two_boxes("ui-box-key");
    // The handle runs along the top of the box.
    ask(&mut h, "ui.click", json!({"x": ox + 160.0, "y": oy + 6.0})).expect("click the handle");
    for _ in 0..6 {
        pump(&mut h);
    }
    let v = look_at(&mut h);
    assert_eq!(v.pointer("/page/selectedContainer").and_then(Value::as_u64), Some(0), "the handle selected the box: {v}");

    ask(&mut h, "ui.key", json!({"key": "Delete"})).expect("press Delete");
    settle(&mut h);
    let v = look_at(&mut h);
    assert_eq!(v.pointer("/page/containers").and_then(Value::as_array).map(Vec::len), Some(1), "the box is gone: {v}");
    assert_eq!(v.pointer("/page/blocks").and_then(Value::as_u64), Some(blocks - 2), "with its blocks");
}

/// Escape lets go of the selection, and nothing is deleted.
#[test]
fn escape_lets_go_of_a_selected_container() {
    let (_t, mut h, blocks, (ox, oy)) = two_boxes("ui-box-escape");
    ask(&mut h, "ui.click", json!({"x": ox + 160.0, "y": oy + 6.0})).expect("click the handle");
    for _ in 0..6 {
        pump(&mut h);
    }
    assert_eq!(look_at(&mut h).pointer("/page/selectedContainer").and_then(Value::as_u64), Some(0), "selected");
    ask(&mut h, "ui.key", json!({"key": "Escape"})).expect("press Escape");
    settle(&mut h);
    let v = look_at(&mut h);
    assert!(v.pointer("/page/selectedContainer").is_none_or(Value::is_null), "no longer selected: {v}");
    assert_eq!(v.pointer("/page/blocks").and_then(Value::as_u64), Some(blocks), "nothing was deleted");
}

/// The first line of words in `blocks` of a page body, to look for in what is left of it.
fn needle(body: &str, from: usize, to: usize) -> String {
    // The page's text has its boxes written in it; the blocks are what is inside them.
    let (blocks, _) = notecraft_formats::boxes::read(notecraft_model::PageFormat::Cmgfm, body);
    let md = notecraft_formats::render::blocks_to_markdown(blocks.get(from..to.min(blocks.len())).unwrap_or_default());
    md.lines().map(|l| l.trim_start_matches(['#', '-', '*', '>', ' ']).trim()).find(|l| l.len() > 6).unwrap_or_default().to_string()
}

fn select_box_by_handle(h: &mut Harness<'_, NoteCraftApp>, ox: f64, oy: f64, x: f64) {
    ask(h, "ui.click", json!({"x": ox + x, "y": oy + 6.0})).expect("click the handle");
    for _ in 0..6 {
        pump(h);
    }
}

/// Delete in a text box is Delete in the text box. A box that is not selected is never taken
/// away by the key, and neither is one whose words are being edited.
#[test]
fn the_delete_key_in_the_words_does_not_delete_a_box() {
    let (_t, mut h, blocks, _) = two_boxes("ui-box-delete-in-text");
    ask(&mut h, "ui.caret", json!({"block": 0})).expect("caret");
    for _ in 0..4 {
        pump(&mut h);
    }
    ask(&mut h, "ui.key", json!({"key": "Delete"})).expect("press Delete");
    settle(&mut h);
    let v = look_at(&mut h);
    assert_eq!(v.pointer("/page/containers").and_then(Value::as_array).map(Vec::len), Some(2), "both boxes are still there: {v}");
    assert_eq!(v.pointer("/page/blocks").and_then(Value::as_u64), Some(blocks), "and every block");
    assert!(v.pointer("/page/selectedContainer").is_none_or(Value::is_null), "nothing was selected");
}

/// A right click with words selected in a box belongs to the words: the page's menu opens, and
/// the box is not selected.
#[test]
fn right_click_with_words_selected_keeps_the_text_menu() {
    let (_t, mut h, _, (ox, oy)) = two_boxes("ui-box-words-menu");
    ask(&mut h, "ui.caret", json!({"block": 0})).expect("caret");
    for _ in 0..4 {
        pump(&mut h);
    }
    ask(&mut h, "ui.key", json!({"key": "Home"})).expect("Home");
    ask(&mut h, "ui.key", json!({"key": "End", "shift": true})).expect("select to the end");
    for _ in 0..4 {
        pump(&mut h);
    }
    let v = look_at(&mut h);
    let (at, anchor) = (v.pointer("/page/caret/at").and_then(Value::as_u64), v.pointer("/page/caret/anchor").and_then(Value::as_u64));
    assert!(at.is_some() && at != anchor, "words are selected before the right click: {v}");

    ask(&mut h, "ui.click", json!({"x": ox + 100.0, "y": oy + 30.0, "button": "right"})).expect("right click");
    for _ in 0..6 {
        pump(&mut h);
    }
    let v = look_at(&mut h);
    assert!(v.pointer("/page/selectedContainer").is_none_or(Value::is_null), "the box was not selected: {v}");
    assert!(h.query_all_by_label("Table").count() >= 1, "the page's menu opened");
    assert_eq!(h.query_all_by_label("Delete").count(), 0, "and it has no box Delete on it");
}

/// Bare paper has the page's menu and nothing about a box.
#[test]
fn right_click_on_bare_paper_opens_only_the_page_menu() {
    let (_t, mut h, _, (ox, oy)) = two_boxes("ui-box-bare-paper");
    ask(&mut h, "ui.click", json!({"x": ox + 100.0, "y": oy + 300.0, "button": "right"})).expect("right click");
    for _ in 0..6 {
        pump(&mut h);
    }
    let v = look_at(&mut h);
    assert!(v.pointer("/page/selectedContainer").is_none_or(Value::is_null), "no box is selected: {v}");
    assert!(h.query_all_by_label("Table").count() >= 1, "the page's menu opened");
    assert_eq!(h.query_all_by_label("Delete").count(), 0, "with no box Delete on it");
    assert_eq!(v.pointer("/page/containers").and_then(Value::as_array).map(Vec::len), Some(2), "and no box was started or lost");
}

/// Deleting a box takes its words out of the saved page too, and leaves the other box's words.
#[test]
fn deleting_a_box_removes_its_words_from_the_saved_page() {
    let (_t, mut h, _, (ox, oy)) = two_boxes("ui-box-saved");
    let before = h.state().body().unwrap_or_default().to_string();
    let gone = needle(&before, 0, 2);
    let kept = needle(&before, 2, usize::MAX);
    assert!(!gone.is_empty() && !kept.is_empty() && gone != kept, "the sample page gives each box something to look for: {gone:?} {kept:?}");
    assert!(before.contains(&gone) && before.contains(&kept));

    select_box_by_handle(&mut h, ox, oy, 160.0);
    ask(&mut h, "ui.key", json!({"key": "Delete"})).expect("press Delete");
    settle(&mut h);
    settle(&mut h);
    let after = h.state().body().unwrap_or_default().to_string();
    assert!(!after.contains(&gone), "the deleted box's words are gone from the page: {gone:?}");
    assert!(after.contains(&kept), "the other box's words stay: {kept:?}");
}

/// Copy puts the box's content on the clipboard and leaves it on the page. Cut does the same and
/// takes the box away.
#[test]
fn copy_and_cut_on_a_box_reach_the_clipboard() {
    for (label, removes) in [("Copy", false), ("Cut", true)] {
        let board = MemoryBoard::new();
        let (_t, mut h, blocks, (ox, oy)) = two_boxes_on(&format!("ui-box-{label}"), &board);
        let before = h.state().body().unwrap_or_default().to_string();
        let words = needle(&before, 0, 2);
        ask(&mut h, "ui.click", json!({"x": ox + 100.0, "y": oy + 30.0, "button": "right"})).expect("right click");
        for _ in 0..6 {
            pump(&mut h);
        }
        // The Home tab has a Cut and a Copy of its own; the box's menu is drawn over it.
        h.query_all_by_label(label).last().expect("the menu item").click();
        for _ in 0..8 {
            pump(&mut h);
        }
        let copied = board.read_text();
        assert!(copied.contains(&words), "{label} put the box's words on the clipboard: {words:?} in {copied:?}");
        settle(&mut h);
        let v = look_at(&mut h);
        let boxes = v.pointer("/page/containers").and_then(Value::as_array).map_or(0, Vec::len);
        if removes {
            assert_eq!(boxes, 1, "Cut took the box away");
            assert_eq!(v.pointer("/page/blocks").and_then(Value::as_u64), Some(blocks - 2));
        } else {
            assert_eq!(boxes, 2, "Copy left the box");
            assert_eq!(v.pointer("/page/blocks").and_then(Value::as_u64), Some(blocks));
        }
    }
}

/// The last box can go, and the page is still a page: bare paper starts a new box.
#[test]
fn deleting_the_only_box_leaves_a_page_that_can_be_written_on() {
    let (_t, mut app, s) = sample_app("ui-box-only");
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    let v = look_at(&mut h);
    assert_eq!(v.pointer("/page/containers").and_then(Value::as_array).map(Vec::len), Some(1), "one box holds the page");
    let origin = v.pointer("/page/origin").and_then(Value::as_array).cloned().unwrap_or_default();
    let (ox, oy) = (origin.first().and_then(Value::as_f64).unwrap_or(0.0), origin.get(1).and_then(Value::as_f64).unwrap_or(0.0));

    select_box_by_handle(&mut h, ox, oy, 100.0);
    ask(&mut h, "ui.key", json!({"key": "Delete"})).expect("press Delete");
    settle(&mut h);
    let v = look_at(&mut h);
    assert_eq!(v.pointer("/page/blocks").and_then(Value::as_u64), Some(0), "the page is empty: {v}");
    assert!(v.pointer("/page/selectedContainer").is_none_or(Value::is_null), "and nothing is selected");

    // Bare paper starts a box where the pointer is, as it does on any empty page.
    ask(&mut h, "ui.click", json!({"x": ox + 200.0, "y": oy + 120.0})).expect("click the paper");
    settle(&mut h);
    let v = look_at(&mut h);
    assert_eq!(v.pointer("/page/blocks").and_then(Value::as_u64), Some(1), "a new block is there to write in: {v}");
    assert!(v.pointer("/page/caret").is_some_and(|c| !c.is_null()), "with the caret in it");
}

/// Clicking into the words of another box lets go of the selection: the words are being edited
/// now, not the box.
#[test]
fn clicking_into_words_lets_go_of_a_selected_box() {
    let (_t, mut h, blocks, (ox, oy)) = two_boxes("ui-box-click-away");
    select_box_by_handle(&mut h, ox, oy, 160.0);
    assert_eq!(look_at(&mut h).pointer("/page/selectedContainer").and_then(Value::as_u64), Some(0), "selected");
    ask(&mut h, "ui.click", json!({"x": ox + 360.0 + 60.0, "y": oy + 30.0 + 40.0})).expect("click the other box's words");
    settle(&mut h);
    let v = look_at(&mut h);
    assert!(v.pointer("/page/selectedContainer").is_none_or(Value::is_null), "no longer selected: {v}");
    assert!(v.pointer("/page/caret").is_some_and(|c| !c.is_null()), "the caret is in the words");
    assert_eq!(v.pointer("/page/blocks").and_then(Value::as_u64), Some(blocks), "nothing was deleted");
}

/// Opens `page` arranged in two boxes: the first holds two blocks, the second the rest. Returns
/// the page's block count and where its first box is drawn.
fn arrange_in_two(h: &mut Harness<'_, NoteCraftApp>, page: &str) -> (u64, (f64, f64)) {
    ask(h, "ui.open", json!({"path": page})).expect("open");
    settle(h);
    let blocks = look_at(h).pointer("/page/blocks").and_then(Value::as_u64).unwrap_or(0);
    assert!(blocks >= 4, "{page} has blocks in it: {blocks}");
    let line = format!("0,0,320,2; 360,30,300,{}", blocks - 2);
    ask(h, "engine.execute", json!({"command": "page.containers", "params": {"path": page, "boxes": line}})).expect("arrange");
    ask(h, "ui.open", json!({"path": page})).expect("open again");
    settle(h);
    let origin = look_at(h).pointer("/page/origin").and_then(Value::as_array).cloned().unwrap_or_default();
    (blocks, (origin.first().and_then(Value::as_f64).unwrap_or(0.0), origin.get(1).and_then(Value::as_f64).unwrap_or(0.0)))
}

/// A selected box belongs to the page it was selected on. Opening another page must not leave a
/// box of that page shaded, with Delete ready to take it.
#[test]
fn a_selected_box_does_not_follow_you_to_another_page() {
    let (_t, mut app, s) = sample_app("ui-box-other-page");
    let md = s.md_page.to_string_lossy().to_string();
    let html = s.html_page.to_string_lossy().to_string();
    app.open_path(&md);
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    let (_, (ox, oy)) = arrange_in_two(&mut h, &md);
    select_box_by_handle(&mut h, ox, oy, 160.0);
    assert_eq!(look_at(&mut h).pointer("/page/selectedContainer").and_then(Value::as_u64), Some(0), "selected on the first page");

    ask(&mut h, "ui.open", json!({"path": html})).expect("open the other page");
    settle(&mut h);
    let v = look_at(&mut h);
    assert!(v.pointer("/page/selectedContainer").is_none_or(Value::is_null), "nothing is selected on the other page: {v}");

    // And Delete there deletes nothing.
    let blocks = v.pointer("/page/blocks").and_then(Value::as_u64);
    ask(&mut h, "ui.key", json!({"key": "Delete"})).expect("press Delete");
    settle(&mut h);
    assert_eq!(look_at(&mut h).pointer("/page/blocks").and_then(Value::as_u64), blocks, "the key deleted nothing");
}

/// Boxes work the same on an HTML page: the box goes, and the saved file loses its words.
#[test]
fn deleting_a_box_on_an_html_page_removes_it_from_the_file() {
    let (_t, mut app, s) = sample_app("ui-box-html");
    let html = s.html_page.to_string_lossy().to_string();
    app.open_path(&html);
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    let (blocks, (ox, oy)) = arrange_in_two(&mut h, &html);
    let file = |s: &Sample| std::fs::read_to_string(s.html_page.join("index.html")).unwrap_or_default();
    assert!(file(&s).contains("Project X"), "the first box holds the title");

    select_box_by_handle(&mut h, ox, oy, 160.0);
    ask(&mut h, "ui.key", json!({"key": "Delete"})).expect("press Delete");
    settle(&mut h);
    settle(&mut h);
    let v = look_at(&mut h);
    assert_eq!(v.pointer("/page/blocks").and_then(Value::as_u64), Some(blocks - 2), "its two blocks are gone: {v}");
    // The page's own title stays in its <title>; what goes is the body's heading and paragraph.
    let saved = file(&s);
    assert!(!saved.contains("<h1>") && !saved.contains("Kickoff notes"), "the first box's words are gone from the saved page:\n{saved}");
    assert!(saved.contains("Quality is not negotiable"), "the other box's words are still saved");
}

/// A box that holds a picture can be deleted like any other, and the page stops naming the file.
#[test]
fn deleting_a_box_with_a_picture_in_it_removes_the_picture_from_the_page() {
    let (_t, mut app, s) = sample_app("ui-box-picture");
    let md = s.md_page.to_string_lossy().to_string();
    app.open_path(&md);
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    let (blocks, (ox, oy)) = arrange_in_two(&mut h, &md);
    let file = |s: &Sample| std::fs::read_to_string(s.md_page.join("index.md")).unwrap_or_default();
    assert!(file(&s).contains("koala.png"), "the sample page has a picture, in the second box");

    // The second box starts at x 360, y 30: its handle is along its top.
    ask(&mut h, "ui.click", json!({"x": ox + 360.0 + 150.0, "y": oy + 30.0 + 6.0})).expect("click the second handle");
    for _ in 0..6 {
        pump(&mut h);
    }
    assert_eq!(look_at(&mut h).pointer("/page/selectedContainer").and_then(Value::as_u64), Some(1), "the second box is selected");
    ask(&mut h, "ui.key", json!({"key": "Delete"})).expect("press Delete");
    settle(&mut h);
    settle(&mut h);
    let v = look_at(&mut h);
    assert_eq!(v.pointer("/page/blocks").and_then(Value::as_u64), Some(2), "only the first box's two blocks are left of {blocks}: {v}");
    assert!(!file(&s).contains("koala.png"), "the saved page no longer names the picture");
}

/// A picture click belongs to the picture, not to its containing page box. This guards the
/// overlapping-hit-test regression where the whole container became selected instead.
#[test]
fn clicking_a_picture_selects_the_picture_only() {
    let (_t, mut app, s) = sample_app("ui-picture-hit-test");
    let md = s.md_page.to_string_lossy().to_string();
    app.open_path(&md);
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    ask(&mut h, "engine.execute", json!({"command": "page.save", "params": {"path": md.clone(), "body": "![the koala](assets/koala.png)\n"}}))
        .expect("write one-picture page");
    ask(&mut h, "engine.execute", json!({"command": "page.containers", "params": {"path": md.clone(), "boxes": "0,0,300,1"}}))
        .expect("arrange the picture");
    ask(&mut h, "ui.open", json!({"path": md})).expect("open the picture page");
    settle(&mut h);
    let origin = look_at(&mut h).pointer("/page/origin").and_then(Value::as_array).cloned().unwrap_or_default();
    let ox = origin.first().and_then(Value::as_f64).unwrap_or(0.0);
    let oy = origin.get(1).and_then(Value::as_f64).unwrap_or(0.0);

    // The picture begins below the container handle/padding.
    ask(&mut h, "ui.click", json!({"x": ox + 120.0, "y": oy + 40.0})).expect("click the picture");
    settle(&mut h);
    let v = look_at(&mut h);
    assert_eq!(v.pointer("/page/selectedImage").and_then(Value::as_u64), Some(0), "the picture is selected: {v}");
    assert!(v.pointer("/page/selectedContainer").is_none_or(Value::is_null), "the containing box is not selected: {v}");

    ask(&mut h, "ui.key", json!({"key": "Escape"})).expect("clear picture selection");
    settle(&mut h);
    assert!(look_at(&mut h).pointer("/page/selectedImage").is_none_or(Value::is_null), "Escape clears the picture selection");

    ask(&mut h, "ui.click", json!({"x": ox + 120.0, "y": oy + 40.0, "button": "right"})).expect("right click the picture");
    settle(&mut h);
    let v = look_at(&mut h);
    assert_eq!(v.pointer("/page/selectedImage").and_then(Value::as_u64), Some(0), "right-clicking selects the picture: {v}");
    assert!(v.pointer("/page/selectedContainer").is_none_or(Value::is_null), "right-clicking the picture does not select its container: {v}");
}

#[test]
fn double_clicking_an_attachment_card_opens_it() {
    let (t, mut app, s) = sample_app("ui-attachment-double-click");
    let page = s.md_page.to_string_lossy().to_string();
    let source = t.path().join("brief.docx");
    std::fs::write(&source, b"attachment").expect("attachment");
    app.open_path(&page);
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    ask(&mut h, "engine.execute", json!({"command": "page.attach", "params": {"path": page, "source": source.to_string_lossy()}})).expect("attach");
    ask(
        &mut h,
        "engine.execute",
        json!({"command": "page.save", "params": {"path": page, "body": "[![brief.docx](assets/brief.docx.png)](assets/brief.docx)"}}),
    )
    .expect("save attachment card");
    ask(&mut h, "engine.execute", json!({"command": "page.containers", "params": {"path": page, "boxes": "0,0,300,1"}}))
        .expect("arrange attachment card");
    ask(&mut h, "ui.open", json!({"path": page})).expect("open");
    settle(&mut h);
    let origin = look_at(&mut h).pointer("/page/origin").and_then(Value::as_array).cloned().unwrap_or_default();
    let ox = origin.first().and_then(Value::as_f64).unwrap_or(0.0);
    let oy = origin.get(1).and_then(Value::as_f64).unwrap_or(0.0);
    ask(&mut h, "ui.click", json!({"x": ox + 60.0, "y": oy + 40.0})).expect("first click card");
    settle(&mut h);
    assert_eq!(h.query_all_by_label_contains("Open attachment").count(), 0, "single click only selects the attachment: {}", look_at(&mut h));
    ask(&mut h, "ui.click", json!({"x": ox + 60.0, "y": oy + 40.0, "count": 2})).expect("double click card");
    settle(&mut h);
    assert!(h.query_all_by_label_contains("Open attachment").count() >= 1, "double click opened the attachment prompt: {}", look_at(&mut h));
}

#[test]
fn delete_and_backspace_remove_a_selected_picture() {
    for key in ["Delete", "Backspace"] {
        let label = if key == "Delete" { "ui-picture-delete" } else { "ui-picture-backspace" };
        let (_t, mut app, s) = sample_app(label);
        let md = s.md_page.to_string_lossy().to_string();
        app.open_path(&md);
        let mut h = harness(app, LIGHT);
        settle(&mut h);
        ask(&mut h, "engine.execute", json!({"command": "page.save", "params": {"path": md.clone(), "body": "![the koala](assets/koala.png)\n"}}))
            .expect("write one-picture page");
        ask(&mut h, "engine.execute", json!({"command": "page.containers", "params": {"path": md.clone(), "boxes": "0,0,300,1"}}))
            .expect("arrange the picture");
        ask(&mut h, "ui.open", json!({"path": md})).expect("open the picture page");
        settle(&mut h);
        let origin = look_at(&mut h).pointer("/page/origin").and_then(Value::as_array).cloned().unwrap_or_default();
        let ox = origin.first().and_then(Value::as_f64).unwrap_or(0.0);
        let oy = origin.get(1).and_then(Value::as_f64).unwrap_or(0.0);
        ask(&mut h, "ui.click", json!({"x": ox + 120.0, "y": oy + 40.0})).expect("click the picture");
        settle(&mut h);
        assert_eq!(look_at(&mut h).pointer("/page/selectedImage").and_then(Value::as_u64), Some(0), "the picture is selected");
        ask(&mut h, "ui.key", json!({"key": key})).expect("remove the picture");
        settle(&mut h);
        assert_eq!(look_at(&mut h).pointer("/page/blocks").and_then(Value::as_u64), Some(0), "{key} removes the selected picture");
    }
}

/// The first drawn line selected whole, then a right click on it: the state the text menu and
/// its toolbar are meant for. Block 0 is the page's title heading, which the window does not draw
/// because the title area above the page already shows it, so the caret lands on the line under
/// it.
fn right_click_on_selected_words(label: &str) -> (TempDir, Harness<'static, NoteCraftApp>) {
    let (t, mut h, _, (ox, oy)) = two_boxes(label);
    ask(&mut h, "ui.caret", json!({"block": 0})).expect("caret");
    for _ in 0..4 {
        pump(&mut h);
    }
    ask(&mut h, "ui.key", json!({"key": "Home"})).expect("Home");
    ask(&mut h, "ui.key", json!({"key": "End", "shift": true})).expect("select to the end");
    for _ in 0..4 {
        pump(&mut h);
    }
    ask(&mut h, "ui.click", json!({"x": ox + 100.0, "y": oy + 30.0, "button": "right"})).expect("right click");
    for _ in 0..8 {
        pump(&mut h);
    }
    (t, h)
}

/// A right click on selected words keeps them selected and floats the mini toolbar over the menu.
#[test]
fn right_click_on_selected_words_keeps_them_selected_and_shows_the_mini_toolbar() {
    let (_t, mut h) = right_click_on_selected_words("ui-minibar-shown");
    let v = look_at(&mut h);
    let (at, anchor) = (v.pointer("/page/caret/at").and_then(Value::as_u64), v.pointer("/page/caret/anchor").and_then(Value::as_u64));
    assert!(at.is_some() && at != anchor, "the words are still selected after the right click: {at:?} {anchor:?}");
    // The Home tab has a Bold, an Italic and the rest of them; the toolbar is drawn over them.
    for name in ["Bold", "Italic", "Strikethrough", "Bullets", "Numbering", "Decrease indent", "Increase indent"] {
        assert!(h.query_all_by_label(name).count() >= 2, "{name} is on the ribbon and on the floating toolbar");
    }
    for name in ["To Do", "Important", "Quote", "Clear Formatting"] {
        assert!(h.query_all_by_label(name).count() >= 1, "{name} is on the toolbar");
    }
}

/// A button on the toolbar acts on the words that were selected when the menu was opened.
#[test]
fn bold_on_the_mini_toolbar_wraps_the_selected_words() {
    let (_t, mut h) = right_click_on_selected_words("ui-minibar-bold");
    let was_bold = h.state().body().unwrap_or_default().matches("**").count();
    h.query_all_by_label("Bold").last().expect("the toolbar's Bold").click();
    settle(&mut h);
    // Whichever words were selected: the page gained a bold run it did not have. Home and
    // Shift+End select the drawn row the caret is on, and a line wraps inside a narrow
    // box, so the test does not spell out where that row ends.
    let body = h.state().body().unwrap_or_default().to_string();
    assert_eq!(body.matches("**").count(), was_bold + 2, "the selected words came back bold:\n{body}");
    assert_eq!(h.query_all_by_label("Italic").count(), 1, "the toolbar closed once a button was pressed");
}

/// The page menu's own Cut works on words that were selected before the right click, which
/// the text box used to forget the moment the button went down.
#[test]
fn cut_on_the_page_menu_cuts_the_words_that_were_selected() {
    let (_t, mut h) = right_click_on_selected_words("ui-minibar-cut");
    let before = h.state().body().unwrap_or_default().to_string();
    assert!(before.contains("NoteCraft pages are"));
    h.query_all_by_label("Cut").last().expect("the menu's Cut").click();
    for _ in 0..8 {
        pump(&mut h);
    }
    settle(&mut h);
    let after = h.state().body().unwrap_or_default().to_string();
    assert!(after.len() < before.len(), "Cut removed the selected words from the page:\n{before}\n---\n{after}");
}

/// Escape closes the menu and the toolbar together.
#[test]
fn escape_closes_the_menu_and_its_toolbar() {
    let (_t, mut h) = right_click_on_selected_words("ui-minibar-escape");
    assert!(h.query_all_by_label("Bold").count() >= 2, "the toolbar is open");
    ask(&mut h, "ui.key", json!({"key": "Escape"})).expect("press Escape");
    settle(&mut h);
    assert_eq!(h.query_all_by_label("Bold").count(), 1, "only the ribbon's Bold is left");
    assert_eq!(h.query_all_by_label("Table").count(), 0, "and the menu is shut");
}

/// A box's menu has no formatting toolbar: with the whole box selected there is no word for Bold
/// to act on, and a button that does nothing is not drawn.
#[test]
fn a_boxs_menu_has_no_formatting_toolbar() {
    let (_t, mut h, _, (ox, oy)) = two_boxes("ui-minibar-box");
    ask(&mut h, "ui.click", json!({"x": ox + 100.0, "y": oy + 30.0, "button": "right"})).expect("right click");
    for _ in 0..8 {
        pump(&mut h);
    }
    assert!(h.query_all_by_label("Delete").count() >= 1, "the box's menu is open");
    assert_eq!(h.query_all_by_label("Bold").count(), 1, "and only the ribbon has a Bold");
}

/// A new page whose body is `body`, opened, and where the first box on it is drawn.
fn blank_page(label: &str, body: &str) -> (TempDir, Harness<'static, NoteCraftApp>, (f64, f64)) {
    let (t, mut app, s) = sample_app(label);
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    ask(
        &mut h,
        "engine.execute",
        json!({"command": "node.create", "params": {"parent": "my-notebook/ideas", "kind": "page", "title": "Blank", "format": "html"}}),
    )
    .expect("create");
    ask(&mut h, "engine.execute", json!({"command": "page.save", "params": {"path": "my-notebook/ideas/blank", "body": body}})).expect("save");
    ask(&mut h, "ui.open", json!({"path": "my-notebook/ideas/blank"})).expect("open");
    settle(&mut h);
    let origin = look_at(&mut h).pointer("/page/origin").and_then(Value::as_array).cloned().unwrap_or_default();
    let at = (origin.first().and_then(Value::as_f64).unwrap_or(0.0), origin.get(1).and_then(Value::as_f64).unwrap_or(0.0));
    (t, h, at)
}

/// Clicking inside an empty box puts the caret there, and what is typed goes in. It used to do
/// nothing at all: a box with no blocks has nothing under the pointer to answer the click.
#[test]
fn clicking_in_an_empty_box_puts_the_caret_there_and_typing_works() {
    for (label, body) in [("empty-p", "<p></p>"), ("nothing", "")] {
        let (_t, mut h, (ox, oy)) = blank_page(&format!("ui-empty-box-{label}"), body);
        let v = look_at(&mut h);
        assert_eq!(v.pointer("/page/blocks").and_then(Value::as_u64), Some(0), "{label}: the page has no blocks, only an empty box");
        assert_eq!(v.pointer("/page/containers").and_then(Value::as_array).map(Vec::len), Some(1), "{label}: and one box");
        // Below the handle, well inside the box.
        ask(&mut h, "ui.click", json!({"x": ox + 100.0, "y": oy + 24.0})).expect("click in the box");
        for _ in 0..6 {
            pump(&mut h);
        }
        let v = look_at(&mut h);
        assert!(v.pointer("/page/caret").is_some_and(|c| !c.is_null()), "{label}: the caret is in the box: {v}");
        assert_eq!(v.pointer("/page/blocks").and_then(Value::as_u64), Some(1), "{label}: the box has a line to write in now");
        assert_eq!(v.pointer("/page/containers").and_then(Value::as_array).map(Vec::len), Some(1), "{label}: still one box");
        ask(&mut h, "ui.text", json!({"text": "hello"})).expect("type");
        settle(&mut h);
        let body = h.state().body().unwrap_or_default().to_string();
        assert!(body.contains("hello"), "{label}: what was typed is on the page:\n{body}");
    }
}

/// The box and its line can be clicked into again after the caret has been somewhere else.
#[test]
fn an_empty_line_can_be_clicked_into_again() {
    let (_t, mut h, (ox, oy)) = blank_page("ui-empty-box-again", "<p></p>");
    ask(&mut h, "ui.click", json!({"x": ox + 100.0, "y": oy + 24.0})).expect("click in");
    for _ in 0..4 {
        pump(&mut h);
    }
    // Leave the line without writing anything: it stays, empty, in the box.
    ask(&mut h, "ui.click", json!({"x": ox + 100.0, "y": oy + 700.0, "button": "right"})).expect("right click away");
    for _ in 0..4 {
        pump(&mut h);
    }
    ask(&mut h, "ui.key", json!({"key": "Escape"})).expect("Escape");
    for _ in 0..4 {
        pump(&mut h);
    }
    ask(&mut h, "ui.click", json!({"x": ox + 100.0, "y": oy + 24.0})).expect("click in again");
    for _ in 0..6 {
        pump(&mut h);
    }
    let v = look_at(&mut h);
    assert!(v.pointer("/page/caret").is_some_and(|c| !c.is_null()), "the caret is back in the box: {v}");
    ask(&mut h, "ui.text", json!({"text": "again"})).expect("type");
    settle(&mut h);
    assert!(h.state().body().unwrap_or_default().contains("again"), "and typing goes in");
}

fn nav(h: &mut Harness<'_, NoteCraftApp>) -> (u64, u64, String) {
    let v = look_at(h);
    (
        v.pointer("/nav/back").and_then(Value::as_u64).unwrap_or(99),
        v.pointer("/nav/forward").and_then(Value::as_u64).unwrap_or(99),
        v.pointer("/nav/current").and_then(Value::as_str).unwrap_or("").to_string(),
    )
}

/// Three pages opened one after another, so there is somewhere to go back to.
fn three_pages(label: &str) -> (TempDir, Harness<'static, NoteCraftApp>, [String; 3]) {
    let (t, mut app, s) = sample_app(label);
    let pages = [s.md_page.to_string_lossy().to_string(), s.html_page.to_string_lossy().to_string(), s.deep_page.to_string_lossy().to_string()];
    app.open_path(&pages[0]);
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    for page in &pages[1..] {
        ask(&mut h, "ui.open", json!({"path": page})).expect("open");
        settle(&mut h);
    }
    (t, h, pages)
}

/// Back and Forward walk the pages in the order they were opened.
#[test]
fn back_and_forward_walk_the_pages_that_were_opened() {
    let (_t, mut h, _) = three_pages("ui-nav-walk");
    let (back, forward, third) = nav(&mut h);
    assert_eq!((back, forward), (2, 0), "two pages behind, none ahead");

    ask(&mut h, "ui.back", json!({})).expect("back");
    settle(&mut h);
    let (back, forward, second) = nav(&mut h);
    assert_eq!((back, forward), (1, 1));
    assert_ne!(second, third, "back is a different page");

    ask(&mut h, "ui.back", json!({})).expect("back");
    settle(&mut h);
    let (back, forward, first) = nav(&mut h);
    assert_eq!((back, forward), (0, 2));
    assert!(first != second && first != third, "three different pages");

    ask(&mut h, "ui.back", json!({})).expect("back at the start does nothing");
    settle(&mut h);
    assert_eq!(nav(&mut h).2, first, "still on the first page");

    ask(&mut h, "ui.forward", json!({})).expect("forward");
    ask(&mut h, "ui.forward", json!({})).expect("forward");
    settle(&mut h);
    assert_eq!(nav(&mut h), (2, 0, third), "forward twice is back where it started");
}

/// Opening a page after going back ends the way forward, as in a browser.
#[test]
fn opening_a_page_after_going_back_ends_the_way_forward() {
    let (_t, mut h, pages) = three_pages("ui-nav-branch");
    ask(&mut h, "ui.back", json!({})).expect("back");
    settle(&mut h);
    assert_eq!(nav(&mut h).1, 1, "one page ahead");
    ask(&mut h, "ui.open", json!({"path": pages[2]})).expect("open another page");
    settle(&mut h);
    assert_eq!(nav(&mut h).1, 0, "a new visit leaves nothing ahead");
}

/// A page deleted since is stepped over, not stopped at.
#[test]
fn back_steps_over_a_page_that_has_been_deleted() {
    let (_t, mut h, pages) = three_pages("ui-nav-gone");
    let gone = pages[1].clone();
    ask(&mut h, "engine.execute", json!({"command": "node.delete", "params": {"path": gone}})).expect("delete the middle page");
    ask(&mut h, "ui.back", json!({})).expect("back");
    settle(&mut h);
    let (_, _, now) = nav(&mut h);
    assert!(now.contains("format-ideas") || now.ends_with("format-ideas") || !now.is_empty(), "landed somewhere");
    let v = look_at(&mut h);
    assert_eq!(v.pointer("/nav/back").and_then(Value::as_u64), Some(0), "the deleted page was dropped from the way back: {v}");
}

/// The Back button on the toolbar, and Alt+Left.
#[test]
fn the_back_button_and_alt_left_go_back() {
    let (_t, mut h, _) = three_pages("ui-nav-button");
    let (_, _, third) = nav(&mut h);
    h.query_all_by_label("Back").last().expect("the Back button").click();
    settle(&mut h);
    let (back, forward, second) = nav(&mut h);
    assert_eq!((back, forward), (1, 1), "the button went back one page");
    assert_ne!(second, third);

    ask(&mut h, "ui.key", json!({"key": "ArrowLeft", "alt": true})).expect("Alt+Left");
    settle(&mut h);
    assert_eq!(nav(&mut h).0, 0, "Alt+Left went back again");

    ask(&mut h, "ui.key", json!({"key": "ArrowRight", "alt": true})).expect("Alt+Right");
    settle(&mut h);
    assert_eq!(nav(&mut h).1, 1, "Alt+Right went forward");
    h.query_all_by_label("Forward").last().expect("the Forward button").click();
    settle(&mut h);
    assert_eq!(nav(&mut h), (2, 0, third), "and the Forward button took it the rest of the way");
}

/// Deleting a box can be taken back, and brought forward again.
#[test]
fn undo_brings_a_deleted_box_back_and_redo_deletes_it_again() {
    let (_t, mut h, blocks, (ox, oy)) = two_boxes("ui-undo-box");
    let original = h.state().body().unwrap_or_default().to_string();
    select_box_by_handle(&mut h, ox, oy, 160.0);
    ask(&mut h, "ui.key", json!({"key": "Delete"})).expect("press Delete");
    settle(&mut h);
    let v = look_at(&mut h);
    assert_eq!(v.pointer("/page/blocks").and_then(Value::as_u64), Some(blocks - 2), "the box is gone");
    assert_eq!(v.pointer("/page/history/canUndo").and_then(Value::as_bool), Some(true), "and there is something to undo");

    ask(&mut h, "ui.undo", json!({})).expect("undo");
    settle(&mut h);
    let v = look_at(&mut h);
    assert_eq!(v.pointer("/page/blocks").and_then(Value::as_u64), Some(blocks), "undo gave the blocks back: {v}");
    assert_eq!(v.pointer("/page/containers").and_then(Value::as_array).map(Vec::len), Some(2), "and the box");
    let now = h.state().body().unwrap_or_default().to_string();
    assert_eq!(notecraft_formats::markdown::parse(&now), notecraft_formats::markdown::parse(&original), "the page has the same blocks as it did");
    assert_eq!(v.pointer("/page/history/canRedo").and_then(Value::as_bool), Some(true));

    ask(&mut h, "ui.redo", json!({})).expect("redo");
    settle(&mut h);
    assert_eq!(look_at(&mut h).pointer("/page/blocks").and_then(Value::as_u64), Some(blocks - 2), "redo deleted it again");
}

/// Ctrl+Z with no text box in use is the page's undo, and the toolbar's Undo does the same.
#[test]
fn ctrl_z_and_the_undo_button_take_back_a_delete() {
    let (_t, mut h, blocks, (ox, oy)) = two_boxes("ui-undo-keys");
    select_box_by_handle(&mut h, ox, oy, 160.0);
    ask(&mut h, "ui.key", json!({"key": "Delete"})).expect("press Delete");
    settle(&mut h);
    ask(&mut h, "ui.key", json!({"key": "Z", "ctrl": true})).expect("Ctrl+Z");
    settle(&mut h);
    assert_eq!(look_at(&mut h).pointer("/page/blocks").and_then(Value::as_u64), Some(blocks), "Ctrl+Z took the delete back");

    ask(&mut h, "ui.key", json!({"key": "Y", "ctrl": true})).expect("Ctrl+Y");
    settle(&mut h);
    assert_eq!(look_at(&mut h).pointer("/page/blocks").and_then(Value::as_u64), Some(blocks - 2), "Ctrl+Y did it again");

    h.query_all_by_label("Undo").last().expect("the Undo button").click();
    settle(&mut h);
    assert_eq!(look_at(&mut h).pointer("/page/blocks").and_then(Value::as_u64), Some(blocks), "so did the toolbar's Undo");
    h.query_all_by_label("Redo").last().expect("the Redo button").click();
    settle(&mut h);
    assert_eq!(look_at(&mut h).pointer("/page/blocks").and_then(Value::as_u64), Some(blocks - 2), "and its Redo");
}

/// The band across the top is the window's title bar, or the system's is: a right click on the
/// bare band says which, and the choice is kept in the settings.
#[test]
fn the_title_bar_is_a_choice_and_is_remembered() {
    let (t, mut h, _) = three_pages("ui-title-bar");
    let settings = |t: &TempDir| std::fs::read_to_string(t.path().join("settings.toml")).unwrap_or_default();
    assert_eq!(look_at(&mut h).pointer("/windowFrame").and_then(Value::as_str), Some("system"), "a test window keeps the system's frame");

    // Bare band: left of the title, right of the Quick Access Toolbar.
    ask(&mut h, "ui.click", json!({"x": 330.0, "y": 14.0, "button": "right"})).expect("right click");
    for _ in 0..6 {
        pump(&mut h);
    }
    h.get_by_label("Use NoteCraft's own title bar").click();
    settle(&mut h);
    assert_eq!(look_at(&mut h).pointer("/windowFrame").and_then(Value::as_str), Some("own"));
    assert!(settings(&t).contains("window_frame = \"own\""), "kept in the settings:\n{}", settings(&t));
    for name in ["Close", "Maximize", "Minimize"] {
        assert!(h.query_all_by_label(name).count() >= 1, "{name} is drawn once the band is the title bar");
    }

    ask(&mut h, "ui.click", json!({"x": 330.0, "y": 14.0, "button": "right"})).expect("right click");
    for _ in 0..6 {
        pump(&mut h);
    }
    h.get_by_label("Use the system title bar").click();
    settle(&mut h);
    assert_eq!(look_at(&mut h).pointer("/windowFrame").and_then(Value::as_str), Some("system"));
    assert!(settings(&t).contains("window_frame = \"system\""), "and back:\n{}", settings(&t));
    assert_eq!(h.query_all_by_label("Close").count(), 0, "no window buttons of ours with the system's frame");
}

/// The menu on the arrow: tick buttons off, move the strip, hide it, and the choices are kept.
#[test]
fn the_quick_access_menu_hides_moves_and_remembers() {
    let (t, mut h, _) = three_pages("ui-qat-menu");
    let settings = |t: &TempDir| std::fs::read_to_string(t.path().join("settings.toml")).unwrap_or_default();
    assert!(h.query_all_by_label("Back").count() >= 1, "the strip is there to begin with");

    // Move it below the ribbon.
    h.get_by_label("Customize Quick Access Toolbar").click();
    for _ in 0..4 {
        pump(&mut h);
    }
    assert!(h.query_all_by_label("Hide Quick Access Toolbar").count() >= 1, "the menu opened");
    h.get_by_label("Show Below the Ribbon").click();
    settle(&mut h);
    let v = look_at(&mut h);
    assert_eq!(v.pointer("/quickAccess/below").and_then(Value::as_bool), Some(true), "it moved: {v}");
    assert!(h.query_all_by_label("Back").count() >= 1, "and is still drawn");
    assert!(settings(&t).contains("qat_below = \"true\""), "the choice is in the settings:\n{}", settings(&t));

    // Take Redo off the strip. The Home tab has a Redo button of its own, which stays.
    let redo_buttons = h.query_all_by_label("Redo").count();
    assert!(redo_buttons >= 2, "a Redo on the strip and one on the Home tab: {redo_buttons}");
    h.get_by_label("Customize Quick Access Toolbar").click();
    for _ in 0..4 {
        pump(&mut h);
    }
    h.query_all_by_label("Redo").last().expect("the menu's Redo").click();
    settle(&mut h);
    let v = look_at(&mut h);
    assert_eq!(v.pointer("/quickAccess/shown/3").and_then(Value::as_bool), Some(false), "Redo is off the strip: {v}");
    assert_eq!(h.query_all_by_label("Redo").count(), redo_buttons - 1, "the strip's Redo is gone and the Home tab's is not");

    // Hide it, and bring it back from the arrow that stays.
    h.get_by_label("Customize Quick Access Toolbar").click();
    for _ in 0..4 {
        pump(&mut h);
    }
    h.get_by_label("Hide Quick Access Toolbar").click();
    settle(&mut h);
    assert_eq!(look_at(&mut h).pointer("/quickAccess/hidden").and_then(Value::as_bool), Some(true));
    assert_eq!(h.query_all_by_label("Back").count(), 0, "no buttons while it is hidden");
    assert!(h.query_all_by_label("Customize Quick Access Toolbar").count() >= 1, "but the arrow is left to bring it back");
    h.get_by_label("Customize Quick Access Toolbar").click();
    for _ in 0..4 {
        pump(&mut h);
    }
    h.get_by_label("Show Quick Access Toolbar").click();
    settle(&mut h);
    assert!(h.query_all_by_label("Back").count() >= 1, "it is back");
    assert!(
        settings(&t).contains("qat_hidden = \"false\"") && settings(&t).contains("qat_redo = \"false\""),
        "and the settings agree:\n{}",
        settings(&t)
    );
}

/// The boxes of an HTML page are elements in its text. The window, the source pane and the saved
/// file all say the same thing, and the page opens in the same boxes again.
#[test]
fn an_html_page_keeps_its_boxes_in_its_text() {
    let (_t, mut app, s) = sample_app("ui-boxes-html-text");
    let html = s.html_page.to_string_lossy().to_string();
    app.open_path(&html);
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    let (blocks, _) = arrange_in_two(&mut h, &html);

    let shown = h.state().body().unwrap_or_default().to_string();
    assert_eq!(shown.matches("class=\"nc-box\"").count(), 2, "the source pane shows one element per box:\n{shown}");
    assert!(shown.contains("position:absolute"), "positioned, so a browser shows the arrangement:\n{shown}");
    let file = std::fs::read_to_string(s.html_page.join("index.html")).unwrap_or_default();
    assert_eq!(file.matches("class=\"nc-box\"").count(), 2, "the saved file has them too:\n{file}");
    assert!(!file.contains("content=\"0,0,320,2"), "and the header line is not used:\n{file}");

    // Opening it again gives the same boxes and the same blocks.
    ask(&mut h, "ui.open", json!({"path": html})).expect("open again");
    settle(&mut h);
    let v = look_at(&mut h);
    assert_eq!(v.pointer("/page/blocks").and_then(Value::as_u64), Some(blocks), "the same blocks: {v}");
    let boxes = v.pointer("/page/containers").and_then(Value::as_array).cloned().unwrap_or_default();
    assert_eq!(boxes.len(), 2, "the same boxes: {boxes:?}");
    assert_eq!(boxes.get(1).and_then(|b| b.get("x")).and_then(Value::as_f64), Some(360.0));
}

/// Writing in a box does not take the boxes out of the text.
#[test]
fn typing_in_a_box_keeps_the_boxes_in_the_text() {
    let (_t, mut h, _, _) = two_boxes("ui-boxes-typing");
    ask(&mut h, "ui.caret", json!({"block": 0})).expect("caret");
    for _ in 0..4 {
        pump(&mut h);
    }
    ask(&mut h, "ui.text", json!({"text": " more words"})).expect("type");
    settle(&mut h);
    settle(&mut h);
    let shown = h.state().body().unwrap_or_default().to_string();
    assert!(shown.contains("more words"), "the words went in:\n{shown}");
    assert_eq!(shown.matches("class=\"nc-box\"").count(), 2, "and both boxes are still in the text:\n{shown}");
}

/// A page nobody arranged stays plain text: no wrapper is written just because it was opened and
/// written on.
#[test]
fn a_page_that_was_never_arranged_stays_plain_text() {
    let (_t, mut app, s) = sample_app("ui-boxes-plain");
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    ask(&mut h, "ui.caret", json!({"block": 0})).expect("caret");
    for _ in 0..4 {
        pump(&mut h);
    }
    ask(&mut h, "ui.text", json!({"text": " edited"})).expect("type");
    settle(&mut h);
    settle(&mut h);
    let file = std::fs::read_to_string(s.md_page.join("index.md")).unwrap_or_default();
    assert!(file.contains("edited"), "the edit was saved:\n{file}");
    assert!(!file.contains("nc-box"), "but no box was written into a page nobody arranged:\n{file}");
}

/// A page from before the boxes were in the text, which kept them on a line in its front matter,
/// still opens in them, and is rewritten in the new form the next time its boxes change.
#[test]
fn an_older_page_is_read_and_moved_to_the_new_form() {
    let (_t, mut app, s) = sample_app("ui-boxes-older");
    let md = s.md_page.to_string_lossy().to_string();
    app.open_path(&md);
    let mut h = harness(app, LIGHT);
    settle(&mut h);
    let blocks = look_at(&mut h).pointer("/page/blocks").and_then(Value::as_u64).unwrap_or(0);
    // Write the older form by hand: a line in the front matter, and a plain body.
    let file_path = s.md_page.join("index.md");
    let text = std::fs::read_to_string(&file_path).unwrap_or_default();
    let line = format!("containers: 40,20,320,2; 380,50,300,{}", blocks - 2);
    let older = text.replacen("\ntitle:", &format!("\n{line}\ntitle:"), 1);
    assert!(older.contains("containers: 40,20"), "the older form was written:\n{older}");
    std::fs::write(&file_path, older).expect("write the older page");
    ask(&mut h, "ui.open", json!({"path": md.clone()})).expect("open");
    settle(&mut h);
    let boxes = look_at(&mut h).pointer("/page/containers").and_then(Value::as_array).cloned().unwrap_or_default();
    assert_eq!(boxes.len(), 2, "it opens in its boxes: {boxes:?}");
    assert_eq!(boxes.first().and_then(|b| b.get("x")).and_then(Value::as_f64), Some(40.0));

    // Moving a box rewrites the page in the new form and drops the old line.
    let v = look_at(&mut h);
    let origin = v.pointer("/page/origin").and_then(Value::as_array).cloned().unwrap_or_default();
    let (ox, oy) = (origin.first().and_then(Value::as_f64).unwrap_or(0.0), origin.get(1).and_then(Value::as_f64).unwrap_or(0.0));
    let (hx, hy) = (ox + 40.0 + 160.0, oy + 20.0 + 6.0);
    ask(&mut h, "ui.press", json!({"x": hx, "y": hy})).expect("press");
    pump(&mut h);
    ask(&mut h, "ui.move", json!({"x": hx + 30.0, "y": hy + 20.0})).expect("move");
    pump(&mut h);
    pump(&mut h);
    ask(&mut h, "ui.release", json!({"x": hx + 30.0, "y": hy + 20.0})).expect("release");
    pump(&mut h);
    settle(&mut h);
    settle(&mut h);
    let now = std::fs::read_to_string(&file_path).unwrap_or_default();
    assert_eq!(now.matches("class=\"nc-box\"").count(), 2, "the boxes are in the text now:\n{now}");
    assert!(!now.contains("containers:"), "and the older line is gone:\n{now}");
}

/// A page with one paragraph of `line` on it, opened and ready to write in.
fn page_of(h: &mut Harness<'_, NoteCraftApp>, s: &Sample, title: &str, line: &str) -> String {
    let section = s.md_page.parent().expect("section").to_string_lossy().into_owned();
    let made =
        ask(h, "engine.execute", json!({"command": "node.create", "params": {"parent": section, "kind": "page", "title": title, "format": "cmgfm"}}))
            .expect("page");
    let page = made.as_str().expect("a path").to_string();
    ask(h, "engine.execute", json!({"command": "page.save", "params": {"path": &page, "body": line}})).expect("save");
    ask(h, "ui.open", json!({"path": &page})).expect("open");
    settle(h);
    page
}

/// Enter at the end of an Arabic paragraph starts a new one, and what is typed next goes
/// into it.
///
/// The owner hit this writing a numbered list in Arabic: lines three, four and five ran
/// together into one paragraph while one and two came out separately. The caret arriving in
/// a right-to-left line was left at character nought instead of at the end of the words, so
/// Enter split the line in front of its first letter: the whole sentence moved down into the
/// new paragraph and the next thing typed ran on to the end of it. A line without
/// right-to-left text in it never showed this, because egui's own text box puts an arriving
/// caret at the end of the line. Both do now.
#[test]
fn enter_in_an_arabic_paragraph_starts_the_next_one() {
    const LINE: &str = "هذا اختبار بسيط";
    for (label, line) in [("arabic", LINE), ("english", "this is a test")] {
        let (_t, app, s) = sample_app("ui-rtl-enter");
        let mut h = harness(app, LIGHT);
        h.run();
        page_of(&mut h, &s, "Enter", line);
        ask(&mut h, "ui.caret", json!({"block": 0})).expect("caret");
        settle(&mut h);

        for n in 1..=3 {
            ask(&mut h, "ui.key", json!({"key": "Enter"})).expect("enter");
            settle(&mut h);
            let caret = ask(&mut h, "ui.inspect", json!({})).expect("inspect").pointer("/page/caret").cloned().unwrap_or(Value::Null);
            let text = caret.get("text").and_then(Value::as_str).unwrap_or("<none>").to_string();
            assert_eq!(text, "", "{label}: Enter {n} left the caret in an empty new paragraph, not holding the old line");
            assert_eq!(caret.get("block").and_then(Value::as_u64), Some(n), "{label}: and the caret is in it");
            ask(&mut h, "ui.text", json!({"text": line})).expect("type");
            settle(&mut h);
        }
        // Four paragraphs, each the line once: nothing ran on to the end of another.
        let body = h.state().body().unwrap_or_default().to_string();
        let written: Vec<&str> = body.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('<')).collect();
        assert_eq!(written, vec![line; 4], "{label}: four separate paragraphs:\n{body}");
    }
}

/// Dragging across an Arabic line selects it, the highlight is drawn, and the words it
/// covers can still be read.
///
/// Three things, because the owner reported all three in turn as "dragging does not
/// select". The drag begins on a paragraph the caret is not in, which is how a reader
/// actually selects: they do not click first.
///
/// 1. **The caret arrives and the selection grows.** A finished block answered a click and
///    nothing answered a press that moved, so a first drag did nothing whatever - no caret,
///    no selection. The drag is sensed under the same id the editor uses for the block, so
///    the caret lands where the press fell and egui carries the drag into the text box.
/// 2. **The highlight is drawn.** Compared against the same page with nothing selected.
/// 3. **The words stay readable.** The fill is opaque and used to be painted after the
///    glyphs, which left a row of blank boxes where the sentence had been.
#[test]
fn a_drag_across_an_arabic_line_selects_it_and_the_words_stay_readable() {
    const LINE: &str = "الملاحظات تعيش أطول من البرنامج";
    let (_t, app, s) = sample_app("ui-rtl-drag");
    let mut h = harness(app, LIGHT);
    h.run();
    page_of(&mut h, &s, "Drag", LINE);

    let rect = h.get_by_label(LINE).rect();
    let y = rect.center().y;
    // The line hangs from the right edge, so the drag runs leftwards from near that edge.
    let (from, to) = (rect.max.x - 30.0, rect.max.x - 170.0);

    // No click first. This is the press, the drag and the release, and nothing else.
    ask(&mut h, "ui.press", json!({"x": from, "y": y})).expect("press");
    settle(&mut h);
    ask(&mut h, "ui.move", json!({"x": to, "y": y})).expect("drag");
    settle(&mut h);
    ask(&mut h, "ui.release", json!({"x": to, "y": y})).expect("release");
    settle(&mut h);

    let caret = ask(&mut h, "ui.inspect", json!({})).expect("inspect").pointer("/page/caret").cloned().unwrap_or(Value::Null);
    assert_eq!(caret.get("block").and_then(Value::as_u64), Some(0), "the drag put the caret in the paragraph it began on");
    let (at, anchor) = (caret.get("at").and_then(Value::as_u64).unwrap_or(0), caret.get("anchor").and_then(Value::as_u64).unwrap_or(0));
    assert_ne!(at, anchor, "and the drag selected something: at {at}, anchor {anchor}");

    let selected = h.render().expect("a picture of the window");
    review(&mut h, "rtl-drag-selection");

    // The same page with nothing selected, to compare against.
    ask(&mut h, "ui.key", json!({"key": "ArrowRight"})).expect("drop the selection");
    settle(&mut h);
    let caret = ask(&mut h, "ui.inspect", json!({})).expect("inspect").pointer("/page/caret").cloned().unwrap_or(Value::Null);
    assert_eq!(caret.get("at").and_then(Value::as_u64), caret.get("anchor").and_then(Value::as_u64), "an arrow without Shift drops the selection");
    let plain = h.render().expect("a picture of the window");

    // The band the drag covered, in the rendered picture's own pixels.
    let scale = selected.width() as f32 / 1200.0;
    let (x0, x1) = ((to * scale) as u32, (from * scale) as u32);
    let (y0, y1) = (((y - 8.0) * scale) as u32, ((y + 8.0) * scale) as u32);
    let (mut changed, mut dark) = (0u32, 0u32);
    for py in y0..y1.min(selected.height()) {
        for px in x0..x1.min(selected.width()) {
            let (a, b) = (selected.get_pixel(px, py), plain.get_pixel(px, py));
            if a != b {
                changed += 1;
            }
            if a[0] < 120 && a[1] < 120 && a[2] < 120 {
                dark += 1;
            }
        }
    }
    assert!(changed > 400, "the highlight is drawn: {changed} pixels differ from the same line unselected");
    // The fill is a pale tint of the page and a glyph is far darker than either, so the
    // band must still hold dark pixels. Measured on 2026-10-19: 225 of them with the fill
    // behind the words, and 23 with it painted over them.
    assert!(dark > 40, "the selected words are still legible under the selection: {dark} dark pixels in the band");
}

/// A drag that leaves the paragraph it began in keeps going: both paragraphs are
/// highlighted, and Copy and Delete act on the whole of it.
///
/// The owner put NoteCraft beside OneNote on 2026-10-19 and this was the difference left:
/// there a drag across two paragraphs highlights both, here it stopped dead at the end of
/// the one it began in. Measured before the change: 2118 pixels of the first paragraph
/// highlighted and 0 of the second.
///
/// A caret belongs to one block, so a selection that leaves it is kept beside the caret as
/// `pageedit::Across` and reported by `ui.inspect` as `across`. While one is open no block
/// is a text box, which is why Delete is answered by the page rather than by egui.
#[test]
fn a_drag_can_select_across_paragraphs() {
    const FIRST: &str = "الملاحظات تعيش أطول من البرنامج";
    const SECOND: &str = "Notes outlive the program that wrote them";
    const THIRD: &str = "سطر ثالث للقياس";
    let body = format!("{FIRST}\n\n{SECOND}\n\n{THIRD}");
    let (_t, app, s) = sample_app("ui-cross");
    let mut h = harness(app, LIGHT);
    h.run();
    page_of(&mut h, &s, "Cross", &body);

    let across = |h: &mut Harness<'_, NoteCraftApp>| -> Value {
        ask(h, "ui.inspect", json!({})).expect("inspect").pointer("/page/across").cloned().unwrap_or(Value::Null)
    };
    let first = h.get_by_label(FIRST).rect();
    let second = h.get_by_label(SECOND).rect();

    // Press in the Arabic paragraph and drag down into the English one.
    ask(&mut h, "ui.press", json!({"x": first.max.x - 60.0, "y": first.center().y})).expect("press");
    settle(&mut h);
    ask(&mut h, "ui.move", json!({"x": second.center().x, "y": second.center().y})).expect("drag down");
    settle(&mut h);
    ask(&mut h, "ui.release", json!({"x": second.center().x, "y": second.center().y})).expect("release");
    settle(&mut h);

    let open = across(&mut h);
    assert_eq!(open.pointer("/from/0").and_then(Value::as_u64), Some(0), "it begins in the paragraph the press fell on: {open}");
    assert_eq!(open.pointer("/to/0").and_then(Value::as_u64), Some(1), "and reaches the one the drag ended in: {open}");
    let selected = h.render().expect("picture");
    review(&mut h, "cross-block");

    // Both paragraphs are drawn with a band behind them. Compared against the same page
    // with nothing selected, which a press on the empty room below the page gives.
    ask(&mut h, "ui.click", json!({"x": 600.0, "y": first.min.y - 40.0})).expect("let go");
    settle(&mut h);
    assert_eq!(across(&mut h), Value::Null, "a press somewhere else lets the selection go");
    let plain = h.render().expect("picture");
    let scale = selected.width() as f32 / 1200.0;
    let band = |rect: egui::Rect| -> u32 {
        let (x0, x1) = ((rect.min.x * scale) as u32, (rect.max.x * scale) as u32);
        let (y0, y1) = ((rect.min.y * scale) as u32, (rect.max.y * scale) as u32);
        let mut n = 0;
        for py in y0..y1.min(selected.height()) {
            for px in x0..x1.min(selected.width()) {
                if selected.get_pixel(px, py) != plain.get_pixel(px, py) {
                    n += 1;
                }
            }
        }
        n
    };
    assert!(band(first) > 400, "the first paragraph is highlighted: {} pixels", band(first));
    assert!(band(second) > 400, "and so is the second, which is the whole point: {} pixels", band(second));

    // Drag it again and Delete: what was covered goes, and the halves either side join.
    ask(&mut h, "ui.press", json!({"x": first.max.x - 60.0, "y": first.center().y})).expect("press");
    settle(&mut h);
    ask(&mut h, "ui.move", json!({"x": second.center().x, "y": second.center().y})).expect("drag down");
    settle(&mut h);
    ask(&mut h, "ui.release", json!({"x": second.center().x, "y": second.center().y})).expect("release");
    settle(&mut h);
    ask(&mut h, "ui.key", json!({"key": "Delete"})).expect("delete");
    settle(&mut h);

    assert_eq!(across(&mut h), Value::Null, "the selection is gone with the words");
    let left = h.state().body().unwrap_or_default().to_string();
    let paragraphs: Vec<&str> = left.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('<')).collect();
    assert_eq!(paragraphs.len(), 2, "three paragraphs became two:\n{left}");
    assert!(paragraphs[0].starts_with("الملاحظات"), "the first keeps what came before the selection: {:?}", paragraphs[0]);
    assert!(paragraphs[0].ends_with("wrote them"), "and is joined to what came after it: {:?}", paragraphs[0]);
    assert_eq!(paragraphs[1], THIRD, "the paragraph below is untouched");
}

/// Ctrl+A, Ctrl+C and Ctrl+V work in an Arabic paragraph, where no egui text box listens.
///
/// The owner reported all three: Ctrl+C did nothing, Ctrl+A selected nothing, and nothing
/// could be pasted. None of those keys was bound anywhere in the window for words. A
/// left-to-right paragraph is an egui text box and egui answers them by itself, which is
/// why it was only ever seen in Arabic.
///
/// Paste is checked against the real clipboard, because `paste_here` reads it directly
/// through `arboard`. Copy cannot be checked the same way: `ctx.copy_text` hands the text
/// to the platform layer, and in a harness there is no platform layer to take it. So the
/// copy half checks that the key reached the handler, and the window itself is where the
/// round trip is seen.
#[test]
fn the_clipboard_keys_work_in_an_arabic_paragraph() {
    const LINE: &str = "الملاحظات تعيش أطول من البرنامج";
    let board = MemoryBoard::new();
    let (_t, app, s) = sample_app("ui-keys");
    let app = app.with_clipboard(Box::new(board.clone()));
    let mut h = harness(app, LIGHT);
    h.run();
    page_of(&mut h, &s, "Keys", LINE);
    ask(&mut h, "ui.caret", json!({"block": 0})).expect("caret");
    settle(&mut h);

    let caret = |h: &mut Harness<'_, NoteCraftApp>| -> (u64, u64) {
        let c = ask(h, "ui.inspect", json!({})).expect("inspect").pointer("/page/caret").cloned().unwrap_or(Value::Null);
        (c.get("at").and_then(Value::as_u64).unwrap_or(0), c.get("anchor").and_then(Value::as_u64).unwrap_or(0))
    };
    let status = |h: &mut Harness<'_, NoteCraftApp>| -> String {
        ask(h, "ui.inspect", json!({})).expect("inspect").pointer("/status/text").and_then(|v| v.as_str()).unwrap_or("").to_string()
    };

    // Ctrl+A takes the whole paragraph.
    ask(&mut h, "ui.key", json!({"key": "A", "ctrl": true})).expect("ctrl+a");
    settle(&mut h);
    let (at, anchor) = caret(&mut h);
    let whole = LINE.chars().count() as u64;
    assert_eq!((at.max(anchor), at.min(anchor)), (whole, 0), "Ctrl+A took the whole paragraph: {at} and {anchor} of {whole}");

    // Ctrl+C reaches the handler, which says so.
    press_copy(&mut h, &board, "C");
    settle(&mut h);
    assert_eq!(status(&mut h), "Copied", "Ctrl+C reached the copy handler");

    // Ctrl+V puts what is on the clipboard into the page.
    const PASTED: &str = "سطر ملصوق";
    board.put_text(PASTED);
    ask(&mut h, "ui.caret", json!({"block": 0})).expect("caret");
    settle(&mut h);
    ask(&mut h, "ui.key", json!({"key": "V", "ctrl": true})).expect("ctrl+v");
    settle(&mut h);
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(body.contains(PASTED), "Ctrl+V put the clipboard into the page:\n{body}");
}

/// Ctrl+V with nothing holding the keyboard, which is the case a picked box leaves.
///
/// The owner copied a box, made an empty one and found paste did nothing. Nothing was
/// listening: boxes answer Ctrl+C and Ctrl+X in `containers.rs` and no key anywhere
/// answered paste, so the ribbon's Paste button was the only way in.
#[test]
fn ctrl_v_works_when_no_paragraph_has_the_keyboard() {
    const PASTED: &str = "pasted with no caret";
    let board = MemoryBoard::new();
    let (_t, app, s) = sample_app("ui-keys-nobody");
    let app = app.with_clipboard(Box::new(board.clone()));
    let mut h = harness(app, LIGHT);
    h.run();
    page_of(&mut h, &s, "NoCaret", "first line");
    // No caret anywhere: nothing has been clicked into.
    let caret = ask(&mut h, "ui.inspect", json!({})).expect("inspect").pointer("/page/caret").cloned().unwrap_or(Value::Null);
    assert_eq!(caret, Value::Null, "nothing holds the keyboard to begin with");

    board.put_text(PASTED);
    ask(&mut h, "ui.key", json!({"key": "V", "ctrl": true})).expect("ctrl+v");
    settle(&mut h);
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(
        body.contains(PASTED),
        "Ctrl+V reached the page with no caret in it:
{body}"
    );
}

/// The Styles list opens from its arrow, and a style picked in it is applied to the line the caret is in.
#[test]
fn the_styles_list_applies_what_is_picked() {
    let (_t, mut app, s) = sample_app("ui-styles-list");
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = sized_harness(app, LIGHT, egui::vec2(2000.0, 900.0));
    settle(&mut h);
    ask(&mut h, "ui.caret", json!({"block": 1})).expect("caret");
    pump(&mut h);
    for (label, starts) in [("Code", "```"), ("Heading 5", "##### "), ("Quote", "> ")] {
        let arrow = h.get_by_label("More styles").rect();
        ask(&mut h, "ui.click", json!({"x": arrow.center().x, "y": arrow.center().y})).expect("open the list");
        for _ in 0..8 {
            pump(&mut h);
        }
        let rows: Vec<_> = h.query_all_by_label(label).collect();
        assert!(!rows.is_empty(), "{label} is in the list");
        let row = rows.last().expect("row").rect();
        ask(&mut h, "ui.click", json!({"x": row.center().x, "y": row.center().y})).expect("pick");
        for _ in 0..8 {
            pump(&mut h);
        }
        settle(&mut h);
        let body = h.state().body().unwrap_or_default().to_string();
        assert!(body.lines().any(|l| l.starts_with(starts)), "{label} was applied:\n{body}");
        ask(&mut h, "ui.caret", json!({"block": 1})).expect("caret again");
        pump(&mut h);
    }
}

/// Selecting a box selects everything in it, so the ribbon's marks, styles, Copy and Cut take all of it.
#[test]
fn a_selected_box_takes_the_ribbon_as_a_whole() {
    let (_t, mut app, s) = sample_app("ui-box-ribbon");
    let page = s.md_page.to_string_lossy().to_string();
    app.open_path(&page);
    let mut h = sized_harness(app, LIGHT, egui::vec2(2000.0, 900.0));
    settle(&mut h);
    let look = |h: &mut Harness<'_, NoteCraftApp>| -> Value { ask(h, "ui.inspect", json!({})).expect("inspect") };
    let blocks = look(&mut h).pointer("/page/blocks").and_then(Value::as_u64).unwrap_or(0);
    let line = format!("40,20,320,{blocks}");
    ask(&mut h, "engine.execute", json!({"command": "page.containers", "params": {"path": page.clone(), "boxes": line}})).expect("arrange");
    ask(&mut h, "ui.open", json!({"path": page.clone()})).expect("open");
    settle(&mut h);
    let click = |h: &mut Harness<'_, NoteCraftApp>, x: f32, y: f32| {
        ask(h, "ui.click", json!({"x": x, "y": y})).expect("click");
        for _ in 0..6 {
            pump(h);
        }
    };
    let click_label = |h: &mut Harness<'_, NoteCraftApp>, label: &str| {
        let r = h.get_by_label(label).rect();
        click(h, r.center().x, r.center().y);
    };

    // Pick the box by its handle.
    let origin = look(&mut h).pointer("/page/origin").and_then(Value::as_array).cloned().unwrap_or_default();
    let (ox, oy) = (origin.first().and_then(Value::as_f64).unwrap_or(0.0) as f32, origin.get(1).and_then(Value::as_f64).unwrap_or(0.0) as f32);
    click(&mut h, ox + 40.0 + 160.0, oy + 20.0 + 6.0);
    assert_eq!(look(&mut h).pointer("/page/selectedContainer").and_then(Value::as_u64), Some(0), "the box is selected");

    // Bold, from the ribbon: every line of words in the box, and the box stays picked.
    click_label(&mut h, "Bold");
    settle(&mut h);
    let body = h.state().body().unwrap_or_default().to_string();
    for bold in [
        "# **Format ideas**",
        "## **A checklist**",
        "- [x] **Create a notebook**",
        "3. **Backup is a copy.**",
        "> **Notes should outlive the app that wrote them.**",
    ] {
        assert!(body.contains(bold), "{bold} is bold, a heading, a list item and a quote alike:\n{body}");
    }
    assert_eq!(look(&mut h).pointer("/page/selectedContainer").and_then(Value::as_u64), Some(0), "still picked after the button");

    // A style from the list goes on every block of the box.
    let arrow = h.get_by_label("More styles").rect();
    click(&mut h, arrow.center().x, arrow.center().y);
    let row = h.query_all_by_label("Heading 3").last().expect("the list").rect();
    click(&mut h, row.center().x, row.center().y);
    settle(&mut h);
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(body.lines().filter(|l| l.starts_with("### ")).count() >= 2, "every block of the box became a heading:\n{body}");

    // A style low in the list lies over the page itself; picking it must still reach the box.
    let arrow = h.get_by_label("More styles").rect();
    click(&mut h, arrow.center().x, arrow.center().y);
    let row = h.query_all_by_label("Code").last().expect("the list").rect();
    assert!(row.center().y > oy, "the row lies over the page: {row:?} against the page at {oy}");
    click(&mut h, row.center().x, row.center().y);
    settle(&mut h);
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(body.contains("```\nFormat ideas\n```"), "the blocks of the box became code:\n{body}");
    assert_eq!(look(&mut h).pointer("/page/selectedContainer").and_then(Value::as_u64), Some(0), "still picked after the list");

    // Cut takes the whole box away with what is in it.
    click_label(&mut h, "Cut");
    settle(&mut h);
    let after = look(&mut h).pointer("/page/blocks").and_then(Value::as_u64).unwrap_or(99);
    assert_eq!(after, 0, "the cut took every block of the box");
}

/// Bold and italic are toggles over a whole line, even one that is partly bold or italic already.
#[test]
fn bold_and_italic_toggle_over_a_line_that_is_partly_marked() {
    let (_t, mut app, s) = sample_app("ui-mark-toggle");
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = sized_harness(app, LIGHT, egui::vec2(2000.0, 900.0));
    settle(&mut h);
    let line = |h: &Harness<'_, NoteCraftApp>| {
        h.state().body().unwrap_or_default().lines().find(|l| l.contains("plain folders")).unwrap_or_default().to_string()
    };
    let select_the_line = |h: &mut Harness<'_, NoteCraftApp>| {
        ask(h, "ui.caret", json!({"block": 1})).expect("caret");
        pump(h);
        ask(h, "ui.key", json!({"key": "A", "ctrl": true})).expect("select all");
        pump(h);
    };
    let before = line(&h);
    assert!(before.contains("**plain files**") && before.contains("*any*"), "the line is partly marked to begin with: {before}");
    for (button, open) in [("Bold", "**"), ("Italic", "*")] {
        select_the_line(&mut h);
        h.get_by_label(button).click();
        h.run();
        let on = line(&h);
        assert!(on.starts_with(open) && on.ends_with(open), "{button} is on the whole line: {on}");
        assert!(!on.contains("****"), "{button} did not double the marks: {on}");
        select_the_line(&mut h);
        h.get_by_label(button).click();
        h.run();
        let off = line(&h);
        assert!(off.contains("plain folders") && !off.starts_with("***"), "{button} came off again: {off}");
    }
}

/// A code block is drawn the same whether or not the caret is in it: same box, same place for the words.
#[test]
fn a_code_block_looks_the_same_when_the_caret_arrives() {
    let (_t, app, s) = sample_app("ui-code-same");
    let file = s.md_page.join("index.md");
    let md = std::fs::read_to_string(&file).expect("page");
    std::fs::write(&file, format!("{md}\n```\nhello code\n```\n")).expect("write");
    let mut app = app;
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = sized_harness(app, LIGHT, egui::vec2(1600.0, 1400.0));
    settle(&mut h);
    let blocks = ask(&mut h, "ui.inspect", json!({})).expect("inspect").pointer("/page/blocks").and_then(Value::as_u64).unwrap_or(0) as usize;
    let words = |h: &Harness<'_, NoteCraftApp>| -> egui::Rect {
        let n = h.query_all_by_label("hello code").next().or_else(|| h.query_all_by_value("hello code").next());
        n.expect("the code is on the page").rect()
    };
    let before = words(&h);
    ask(&mut h, "ui.caret", json!({"block": blocks - 1})).expect("caret");
    for _ in 0..4 {
        pump(&mut h);
    }
    let after = words(&h);
    assert!((before.left() - after.left()).abs() < 1.5, "the words stay where they were across: {before:?} then {after:?}");
    assert!((before.top() - after.top()).abs() < 1.5, "and down: {before:?} then {after:?}");
}

/// Numbered code shows its line numbers on the page and none while it is being written in.
#[test]
fn numbered_code_shows_its_numbers_in_view_and_hides_them_in_edit() {
    let (_t, app, s) = sample_app("ui-code-numbers");
    let file = s.md_page.join("index.md");
    let md = std::fs::read_to_string(&file).expect("page");
    std::fs::write(&file, format!("{md}\n```123|rust\nline one\nline two\nline three\n```\n")).expect("write");
    let mut app = app;
    app.open_path(&s.md_page.to_string_lossy());
    let mut h = sized_harness(app, LIGHT, egui::vec2(1600.0, 1400.0));
    settle(&mut h);
    assert_eq!(h.query_all_by_label("1\n2\n3").count(), 1, "the numbers are drawn beside the code");
    assert!(h.query_all_by_label("123|rust").count() == 0 && h.query_all_by_label("rust").count() >= 1, "the label is the language alone");
    let blocks = ask(&mut h, "ui.inspect", json!({})).expect("inspect").pointer("/page/blocks").and_then(Value::as_u64).unwrap_or(0) as usize;
    ask(&mut h, "ui.caret", json!({"block": blocks - 1})).expect("caret");
    for _ in 0..4 {
        pump(&mut h);
    }
    assert_eq!(h.query_all_by_label("1\n2\n3").count(), 0, "no numbers while it is written in");
    let file = std::fs::read_to_string(s.md_page.join("index.md")).unwrap_or_default();
    assert!(file.contains("```123|rust"), "the page keeps saying so");
}

/// Direction, tags and the lists reach every line of a selected box, not only the line the caret was in.
#[test]
fn a_selected_box_takes_direction_tags_and_lists_as_a_whole() {
    let (_t, mut app, s) = sample_app("ui-box-ribbon-more");
    let page = s.md_page.to_string_lossy().to_string();
    app.open_path(&page);
    let mut h = sized_harness(app, LIGHT, egui::vec2(2000.0, 900.0));
    settle(&mut h);
    let look = |h: &mut Harness<'_, NoteCraftApp>| -> Value { ask(h, "ui.inspect", json!({})).expect("inspect") };
    let blocks = look(&mut h).pointer("/page/blocks").and_then(Value::as_u64).unwrap_or(0);
    ask(&mut h, "engine.execute", json!({"command": "page.containers", "params": {"path": page.clone(), "boxes": format!("40,20,320,{blocks}")}}))
        .expect("arrange");
    ask(&mut h, "ui.open", json!({"path": page.clone()})).expect("open");
    settle(&mut h);
    let click = |h: &mut Harness<'_, NoteCraftApp>, x: f32, y: f32| {
        ask(h, "ui.click", json!({"x": x, "y": y})).expect("click");
        for _ in 0..6 {
            pump(h);
        }
    };
    let origin = look(&mut h).pointer("/page/origin").and_then(Value::as_array).cloned().unwrap_or_default();
    let (ox, oy) = (origin.first().and_then(Value::as_f64).unwrap_or(0.0) as f32, origin.get(1).and_then(Value::as_f64).unwrap_or(0.0) as f32);
    click(&mut h, ox + 40.0 + 160.0, oy + 20.0 + 6.0);
    assert_eq!(look(&mut h).pointer("/page/selectedContainer").and_then(Value::as_u64), Some(0), "the box is selected");

    // Right to left on every paragraph of words in it.
    let r = h.get_by_label("Paragraph right to left").rect();
    click(&mut h, r.center().x, r.center().y);
    settle(&mut h);
    let body = h.state().body().unwrap_or_default().to_string();
    let marked =
        body.lines().filter(|l| l.contains("Create a notebook") || l.contains("Backup is a copy") || l.contains("Notes should outlive")).count();
    let with_mark = body.lines().filter(|l| l.contains('\u{200F}')).count();
    assert!(marked >= 3 && with_mark >= 6, "every line of words in the box reads right to left ({with_mark} marked):\n{body}");
    assert!(!body.contains("\u{200F}graph TD") && !body.contains("\u{200F}\\int"), "a diagram and an equation are left alone:\n{body}");

    // A tag goes on every line, and the same button takes it off every line.
    let tagged = |h: &Harness<'_, NoteCraftApp>| h.state().body().unwrap_or_default().lines().filter(|l| l.contains("#important")).count();
    let r = h.get_by_label("Important (Ctrl+2)").rect();
    click(&mut h, r.center().x, r.center().y);
    settle(&mut h);
    assert!(tagged(&h) >= 6, "the tag went on all of them: {}", tagged(&h));
    let r = h.get_by_label("Important (Ctrl+2)").rect();
    click(&mut h, r.center().x, r.center().y);
    settle(&mut h);
    assert_eq!(
        tagged(&h),
        0,
        "and came off all of them:
{}",
        h.state().body().unwrap_or_default()
    );
}

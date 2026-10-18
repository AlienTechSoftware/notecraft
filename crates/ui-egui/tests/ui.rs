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
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Duration;

const LIGHT: Theme = Theme { mode: Mode::Light, accent: Accent::Default };
const DARK: Theme = Theme { mode: Mode::Dark, accent: Accent::Default };

fn params(pairs: &[(&str, &str)]) -> Params {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

fn session(t: &TempDir) -> Session {
    Session::new(&t.path().join("notebooks"), Settings::at(t.path().join("settings.toml"))).expect("session")
}

fn sample_app(label: &str) -> (TempDir, NoteCraftApp, Sample) {
    let t = TempDir::new(label);
    let mut s = session(&t);
    let sample = create_sample(&mut s, Some(LOGO_PNG)).expect("sample");
    let mut app = NoteCraftApp::new(s);
    app.set_redact_paths(true);
    (t, app, sample)
}

fn harness(mut app: NoteCraftApp, theme: Theme) -> Harness<'static, NoteCraftApp> {
    app.set_theme(theme);
    Harness::builder().with_size(egui::vec2(1200.0, 760.0)).wgpu().build_ui_state(|ui, app: &mut NoteCraftApp| app.show(ui), app)
}

/// Feeds the clicks, keys and text that control requests queued into the next frame, then runs it.
fn pump(h: &mut Harness<'_, NoteCraftApp>) {
    let events = h.state_mut().take_events();
    h.input_mut().events.extend(events);
    h.step();
}

/// Steps until pictures loaded in the background have arrived.
fn settle(h: &mut Harness<'_, NoteCraftApp>) {
    for _ in 0..25 {
        pump(h);
        std::thread::sleep(Duration::from_millis(20));
    }
    h.run();
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
fn clicking_a_page_opens_it_and_a_container_toggles() {
    let (_t, mut app, _s) = sample_app("ui-click");
    app.expand_all();
    let mut h = harness(app, LIGHT);
    h.run();
    h.get_by_label("Project X").click();
    h.run();
    assert!(h.state().body().is_some_and(|b| b.contains("Kickoff notes")), "the HTML page is open");
    h.get_by_label("Format ideas").click();
    h.run();
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
            assert!(after.starts_with('#'), "{label}: still Markdown:\n{after}");
        } else {
            assert!(after.contains("<h1>") && after.contains("<p>"), "{label}: still HTML:\n{after}");
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
    // The table in the sample page is a block the caret cannot enter.
    let rows = ask(&mut h, "ui.inspect", json!({})).expect("inspect")["page"]["blocks"].as_u64().unwrap_or(0);
    let mut refused = 0;
    for n in 0..rows {
        if ask(&mut h, "ui.caret", json!({"block": n})).is_err() {
            refused += 1;
        }
    }
    assert!(refused > 0, "the table should refuse the caret");
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
    h.run();
    h.get_by_label("Right-to-Left").click();
    h.run();
    h.run();
    let body = h.state().body().unwrap_or_default().to_string();
    let marked: Vec<&str> = body.lines().filter(|l| l.starts_with('\u{200F}')).collect();
    assert_eq!(marked.len(), 1, "one paragraph carries the mark:\n{body:?}");
    h.get_by_label("Alignment and direction").click();
    h.run();
    h.get_by_label("Direction from the words").click();
    h.run();
    h.run();
    assert!(!h.state().body().unwrap_or_default().contains('\u{200F}'), "auto takes the mark off");

    // The run direction is behind the launcher, with the options that are not built.
    h.get_by_label("Advanced direction").click();
    h.run();
    h.get_by_label("Run right to left (Alt+Shift+R)").click();
    h.run();
    h.run();
    let body = h.state().body().unwrap_or_default().to_string();
    assert!(body.contains('\u{2067}') && body.contains('\u{2069}'), "an isolate went in:\n{body:?}");

    // The page control writes into the page's own file, where it travels with the folder.
    h.get_by_label("Page right to left").click();
    h.run();
    h.run();
    let file = std::fs::read_to_string(s.md_page.join("index.md")).unwrap_or_default();
    assert!(file.contains("dir: rtl"), "the page's own direction is in its front matter:\n{file}");
    review(&mut h, "rtl");
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
    assert!(file.contains("containers:"), "and it is written in the page:\n{file}");
}

/// The side panel can be split the way OneNote splits it: notebooks and sections in one
/// column, the pages of the chosen section in the next, and the three-bar button hides the
/// first of them.
#[test]
fn the_notebooks_and_the_pages_can_have_columns_of_their_own() {
    let (_t, mut app, s) = sample_app("ui-panes");
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
    assert_eq!(panes(&mut h), (false, true), "one tree to begin with");

    // The three-bar button: first press splits the panel and hides the notebooks.
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
    h.get_by_label("Insert").click();
    h.run();
    h.get_by_label("Table").click();
    h.run();
    assert!(h.state().body().is_some_and(|b| b.contains("| A | B | C |")), "{:?}", h.state().body());
    assert!(h.state().is_dirty());
    h.state_mut().save();
    assert!(!h.state().is_dirty());
    review(&mut h, "ribbon_insert");

    h.state_mut().open_path(&s.html_page.to_string_lossy());
    h.run();
    h.get_by_label("Table").click();
    h.run();
    assert!(h.state().body().is_some_and(|b| b.contains("<table>") && b.contains("<th>A</th>")), "{:?}", h.state().body());
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
    assert_eq!(h.state().status(), "Added the welcome notebook");
    assert!(h.state().body().is_some_and(|b| b.starts_with("# Welcome to NoteCraft")), "opened on the start page: {:?}", h.state().body());
    review(&mut h, "welcome_start");

    // The start page links to the English group, and the group opens on its first page.
    // "English" is on screen twice, once as a row of the tree and once in the page, so the
    // one further right is the page's own, and the click lands on its first word.
    let rect =
        h.get_all_by_label_contains("English").map(|n| n.rect()).max_by(|a, b| a.min.x.total_cmp(&b.min.x)).expect("the link on the start page");
    ask(&mut h, "ui.click", json!({"x": rect.min.x + 20.0, "y": rect.center().y})).expect("click");
    for _ in 0..6 {
        pump(&mut h);
    }
    assert!(h.state().body().is_some_and(|b| b.starts_with("# Welcome\n")), "the English welcome page: {:?}", h.state().body());

    h.state_mut().add_welcome();
    settle(&mut h);
    assert_eq!(h.state().session().tree().len(), 1, "no second copy");
    assert_eq!(h.state().status(), "The welcome notebook is already here");
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

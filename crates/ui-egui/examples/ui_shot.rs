//! Headless UI screenshots: renders the whole NoteCraft window off screen (wgpu), so it works
//! with a locked screen or a hidden window and never takes focus.
//!
//! `cargo run -p notecraft-ui-egui --example ui_shot -- script.jsonl`
//!
//! The script is JSON lines:
//! - control requests, `{"method": "...", "params": {...}}` (see `docs/control-protocol.md`);
//!   the reply is printed;
//! - `{"shot": "/abs/out.png"}` saves the window as a PNG;
//! - `{"steps": n}` runs more frames, `{"wait": ms}` keeps running frames for a while;
//! - options, anywhere before the first other line: `{"width": 1440, "height": 900,
//!   "scale": 2.0, "empty": true, "theme": "dark-green", "notebook": "docs/demo/DemoNotebook"}`.
//!   The window opens on the sample notebooks unless `empty` is true; `notebook` copies that
//!   notebook folder into the window's own temporary notebooks folder as well (the original
//!   is never touched). Paths are redacted (`<notebooks>/...`).

use egui_kittest::Harness;
use notecraft_engine::sample::create_sample;
use notecraft_engine::{Params, Session};
use notecraft_store::Settings;
use notecraft_store::testing::TempDir;
use notecraft_ui_egui::control::ControlRequest;
use notecraft_ui_egui::theme::Theme;
use notecraft_ui_egui::{LOGO_PNG, NoteCraftApp};
use serde_json::Value;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn step(h: &mut Harness<'_, NoteCraftApp>) {
    let events = h.state_mut().take_events();
    h.input_mut().events.extend(events);
    h.step();
}

fn step_n(h: &mut Harness<'_, NoteCraftApp>, n: usize) {
    for _ in 0..n {
        step(h);
    }
}

/// Copies a folder tree, so a notebook from the repository can be opened without being
/// written to.
fn copy_dir(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: ui_shot script.jsonl");
        std::process::exit(2);
    };
    let script = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            std::process::exit(2);
        }
    };
    let lines: Vec<Value> = script
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            serde_json::from_str(l).unwrap_or_else(|e| {
                eprintln!("bad JSON line {l:?}: {e}");
                std::process::exit(2);
            })
        })
        .collect();
    let opt = |k: &str| lines.iter().find_map(|l| l.get(k).cloned());
    let width = opt("width").and_then(|v| v.as_f64()).unwrap_or(1440.0) as f32;
    let height = opt("height").and_then(|v| v.as_f64()).unwrap_or(900.0) as f32;
    let scale = opt("scale").and_then(|v| v.as_f64()).unwrap_or(2.0) as f32;
    let empty = opt("empty").and_then(|v| v.as_bool()).unwrap_or(false);
    let backstage = opt("backstage").and_then(|v| v.as_str().map(str::to_string));
    let open = opt("open").and_then(|v| v.as_str().map(str::to_string));
    let demo_progress = opt("demo_progress").and_then(|v| v.as_bool()).unwrap_or(false);

    let dir = TempDir::new("ui-shot");
    let notebooks = dir.path().join("notebooks");
    let copied_notebook = if let Some(src) = opt("notebook").and_then(|v| v.as_str().map(std::path::PathBuf::from)) {
        let name = src.file_name().map(|n| n.to_os_string()).unwrap_or_else(|| "notebook".into());
        let copied = notebooks.join(name);
        if let Err(e) = copy_dir(&src, &copied) {
            eprintln!("cannot copy {}: {e}", src.display());
            std::process::exit(2);
        }
        Some(copied)
    } else {
        None
    };
    let mut session = Session::new(&notebooks, Settings::at(dir.path().join("settings.toml"))).expect("session");
    if !empty {
        create_sample(&mut session, Some(LOGO_PNG)).expect("sample notebooks");
    }
    session.refresh();
    if let Some(notebook) = copied_notebook {
        session
            .run("notebook.open", &Params::from([(String::from("path"), notebook.to_string_lossy().into_owned())]))
            .expect("open supplied notebook");
    }
    let (tx, rx) = mpsc::channel::<ControlRequest>();
    let mut app = NoteCraftApp::new(session).with_requests(rx);
    // The screenshots show the window as it is run: with the band as its title bar.
    app.set_frameless(true);
    app.set_redact_paths(true);
    if let Some(t) = opt("theme").and_then(|v| v.as_str().map(str::to_string)) {
        match Theme::parse(&t) {
            Some(theme) => app.set_theme(theme),
            None => eprintln!("unknown theme {t}; using the default"),
        }
    }
    if let Some(page) = backstage {
        app.open_backstage_page(Box::leak(page.into_boxed_str()));
    }
    if let Some(page) = open {
        app.open_path(&page);
    }
    if demo_progress {
        app.show_demo_import_progress(3, 33, "Journal (On 10-12-2026).one");
    }
    let mut harness = Harness::builder()
        .with_size(egui::vec2(width, height))
        .with_pixels_per_point(scale)
        .with_max_steps(1_000_000)
        .wgpu()
        .build_ui_state(|ui, app: &mut NoteCraftApp| app.show(ui), app);
    step_n(&mut harness, 6);
    for l in lines {
        if let Some(p) = l.get("shot").and_then(Value::as_str) {
            step_n(&mut harness, 4);
            match harness.render() {
                Ok(img) => match img.save(p) {
                    Ok(()) => println!("{p}"),
                    Err(e) => eprintln!("cannot save {p}: {e}"),
                },
                Err(e) => eprintln!("render failed: {e}"),
            }
        } else if let Some(n) = l.get("steps").and_then(Value::as_u64) {
            step_n(&mut harness, n as usize);
        } else if let Some(ms) = l.get("wait").and_then(Value::as_u64) {
            let end = Instant::now() + Duration::from_millis(ms);
            while Instant::now() < end {
                step(&mut harness);
                std::thread::sleep(Duration::from_millis(16));
            }
        } else if let Some(m) = l.get("method").and_then(Value::as_str) {
            let (req, reply) = ControlRequest::new(m, l.get("params").cloned().unwrap_or_default());
            if tx.send(req).is_err() {
                eprintln!("the window is gone");
                break;
            }
            step_n(&mut harness, 2);
            match reply.try_recv() {
                Ok(Ok(v)) => println!("{m}: {}", v.to_string().chars().take(200).collect::<String>()),
                Ok(Err(e)) => println!("{m}: error: {e}"),
                Err(_) => println!("{m}: no answer"),
            }
            step_n(&mut harness, 2);
        }
    }
}

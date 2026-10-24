#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
// A release build on Windows is a window app. Without this a console opens beside the window.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
//! NoteCraft desktop app: a native window over the engine.
//!
//! Usage: `notecraft [--notebooks FOLDER] [--open PAGE] [--expand-all] [--control PORT]`. The notebooks folder defaults to the
//! `NOTECRAFT_NOTEBOOKS` environment variable, then `Documents/NoteCraft` in the home folder.

mod applog;
mod telemetry;

use notecraft_engine::Session;
use notecraft_store::Settings;
use notecraft_ui_egui::NoteCraftApp;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::path::PathBuf;

struct Shell {
    app: NoteCraftApp,
    _input_language_watcher: Option<notecraft_ui_egui::input_language::Watcher>,
}

impl eframe::App for Shell {
    /// Puts the clicks, keys and text that control requests queued into the next frame.
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        raw_input.events.extend(self.app.take_events());
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.show(ui);
    }
}

fn notebooks_dir(args: &[String]) -> PathBuf {
    if let Some(i) = args.iter().position(|a| a == "--notebooks") {
        return args.get(i + 1).map(PathBuf::from).unwrap_or_else(notecraft_store::settings::default_notebooks_dir);
    }
    if let Some(saved) = Settings::default_location().get("last_notebooks_root") {
        return PathBuf::from(saved);
    }
    notecraft_store::settings::default_notebooks_dir()
}

/// The first page inside a notebook folder, walking its sections in tree order.
fn first_page_in(notebook: &std::path::Path) -> Option<PathBuf> {
    fn walk(n: &notecraft_store::tree::Node) -> Option<PathBuf> {
        if n.info.kind == notecraft_store::NodeKind::Page {
            return Some(n.path.clone());
        }
        n.children.iter().find_map(walk)
    }
    notecraft_store::tree::load_tree(notebook).as_ref().and_then(walk)
}

fn flag_value(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn icon() -> Option<egui::IconData> {
    let img = image::load_from_memory(notecraft_ui_egui::LOGO_PNG).ok()?.into_rgba8();
    let (width, height) = img.dimensions();
    Some(egui::IconData { rgba: img.into_raw(), width, height })
}

/// Says why the app cannot start. A release build on Windows has no console, so the message
/// also goes into a box; otherwise the window would just never appear.
fn fail(message: &str) -> ! {
    eprintln!("{message}");
    rfd::MessageDialog::new().set_level(rfd::MessageLevel::Error).set_title("NoteCraft").set_description(message).show();
    std::process::exit(1);
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let settings_dir = Settings::default_location().path().parent().map(std::path::Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    applog::init(settings_dir.clone());
    let telemetry_level =
        Settings::default_location().get("telemetry_level").as_deref().and_then(telemetry::Level::parse).unwrap_or_else(telemetry::Level::for_build);
    let telemetry = (telemetry_level != telemetry::Level::None)
        .then(|| telemetry::Session::start(telemetry::telemetry_root(&settings_dir), telemetry_level).ok())
        .flatten();
    if let Some(session) = &telemetry {
        let _ = Settings::default_location().set("telemetry_session_id", session.id());
        let _ = session.record(telemetry::Level::Usage, "app_start", &[("version", env!("CARGO_PKG_VERSION"))]);
    }
    applog::info(&format!(
        "start: NoteCraft {} exe={:?} args={args:?} settings={:?}",
        env!("CARGO_PKG_VERSION"),
        std::env::current_exe().ok(),
        Settings::default_location().path()
    ));
    if args.iter().any(|a| a == "--version") {
        println!("notecraft {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let root = notebooks_dir(&args);
    applog::info(&format!("notebooks folder: {}", root.display()));
    let mut session = match Session::new(&root, Settings::default_location()) {
        Ok(s) => s,
        Err(e) => {
            applog::line("ERROR", &format!("cannot open the notebooks folder {}: {e}", root.display()));
            fail(&format!("NoteCraft cannot open the notebooks folder {}: {e}", root.display()));
        }
    };
    // An explicit command-line root becomes the next launch's remembered notebook root.
    // This prevents a demo or another deliberately selected notebook from being replaced
    // by the old default folder on the next start.
    if args.iter().any(|arg| arg == "--notebooks") {
        let remembered = std::fs::canonicalize(&root).unwrap_or_else(|_| root.clone());
        let _ = session.settings.set("last_notebooks_root", &remembered.to_string_lossy());
    }
    // The first start, with nothing to show: the Welcome notebook, a tour of the app, goes
    // into the empty folder and the window opens on it. Once only: a folder the user emptied
    // on purpose stays empty, and the File screen can add the notebook again by hand.
    let mut open_first: Option<String> = None;
    if session.tree().is_empty() && session.settings.get("welcome_added").is_none() {
        match session.run("notebook.welcome", &notecraft_engine::Params::default()) {
            Ok(notecraft_engine::Output::Path(dir)) => {
                open_first = first_page_in(&dir).map(|p| p.to_string_lossy().into_owned());
            }
            Ok(_) => {}
            Err(e) => eprintln!("NoteCraft: the welcome notebook was not added: {e}"),
        }
        if let Err(e) = session.settings.set("welcome_added", "true") {
            eprintln!("NoteCraft: cannot remember that the welcome notebook was offered: {e}");
        }
    }
    // NoteCraft draws its own title bar unless the settings say the system's is wanted.
    let own_frame = session.settings.get("window_frame").as_deref() != Some("system");
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([820.0, 520.0])
        .with_title("NoteCraft")
        .with_decorations(!own_frame);
    if let Some(icon) = icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    applog::info(&format!("session open ({} tree nodes); starting the window", session.tree().len()));
    let open = flag_value(&args, "--open");
    let expand = args.iter().any(|a| a == "--expand-all");
    let control_port: Option<u16> =
        flag_value(&args, "--control").or_else(|| std::env::var("NOTECRAFT_CONTROL_PORT").ok()).and_then(|p| p.parse().ok());
    let key_dir = Settings::default_location().path().parent().map(std::path::Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let result = eframe::run_native(
        "NoteCraft",
        options,
        Box::new(move |cc| {
            let mut app = NoteCraftApp::new(session);
            let input_language_watcher = cc.window_handle().ok().and_then(|window| match window.as_raw() {
                RawWindowHandle::Win32(handle) => {
                    let ctx = cc.egui_ctx.clone();
                    notecraft_ui_egui::input_language::watch_changes(handle.hwnd.get(), move || ctx.request_repaint())
                }
                _ => None,
            });
            // With no frame of the system's own, the band across the top is the title bar.
            app.set_frameless(own_frame);
            if let Some(port) = control_port {
                match notecraft_ui_egui::control::start(port, &key_dir, cc.egui_ctx.clone()) {
                    Ok((server, rx)) => {
                        println!("NoteCraft control channel on 127.0.0.1:{}; key in {}", server.port, server.key_file.display());
                        app = app.with_control(server, rx);
                    }
                    Err(e) => eprintln!("NoteCraft: the control channel is off: {e}"),
                }
            }
            if expand {
                app.expand_all();
            }
            if let Some(p) = open.as_ref().or(open_first.as_ref()) {
                app.open_path(p);
            }
            applog::info("window ready");
            Ok(Box::new(Shell { app, _input_language_watcher: input_language_watcher }))
        }),
    );
    match &result {
        Ok(()) => applog::info("exit: the window was closed"),
        Err(e) => applog::line("ERROR", &format!("exit: the window failed: {e}")),
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_notebooks_flag_wins() {
        let args = vec!["--notebooks".to_string(), "somewhere".to_string()];
        assert_eq!(notebooks_dir(&args), PathBuf::from("somewhere"));
    }

    #[test]
    fn a_trailing_flag_without_a_value_falls_back_to_the_default() {
        let args = vec!["--notebooks".to_string()];
        assert_eq!(notebooks_dir(&args), notecraft_store::settings::default_notebooks_dir());
    }

    #[test]
    fn the_icon_decodes() {
        let i = icon().expect("icon");
        assert_eq!((i.width, i.height), (256, 256));
        assert_eq!(i.rgba.len(), 256 * 256 * 4);
    }
}

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! NoteCraft desktop app: a native window over the engine.
//!
//! Usage: `notecraft [--notebooks FOLDER] [--open PAGE] [--expand-all] [--control PORT]`. The notebooks folder defaults to the
//! `NOTECRAFT_NOTEBOOKS` environment variable, then `Documents/NoteCraft` in the home folder.

use notecraft_engine::Session;
use notecraft_store::Settings;
use notecraft_ui_egui::NoteCraftApp;
use std::path::PathBuf;

struct Shell {
    app: NoteCraftApp,
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
    match args.iter().position(|a| a == "--notebooks") {
        Some(i) => args.get(i + 1).map(PathBuf::from).unwrap_or_else(notecraft_store::settings::default_notebooks_dir),
        None => notecraft_store::settings::default_notebooks_dir(),
    }
}

fn flag_value(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn icon() -> Option<egui::IconData> {
    let img = image::load_from_memory(notecraft_ui_egui::LOGO_PNG).ok()?.into_rgba8();
    let (width, height) = img.dimensions();
    Some(egui::IconData { rgba: img.into_raw(), width, height })
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = notebooks_dir(&args);
    let session = match Session::new(&root, Settings::default_location()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("NoteCraft cannot open the notebooks folder {}: {e}", root.display());
            std::process::exit(1);
        }
    };
    let mut viewport = egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]).with_min_inner_size([820.0, 520.0]).with_title("NoteCraft");
    if let Some(icon) = icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    let open = flag_value(&args, "--open");
    let expand = args.iter().any(|a| a == "--expand-all");
    let control_port: Option<u16> =
        flag_value(&args, "--control").or_else(|| std::env::var("NOTECRAFT_CONTROL_PORT").ok()).and_then(|p| p.parse().ok());
    let key_dir = Settings::default_location().path().parent().map(std::path::Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    eframe::run_native(
        "NoteCraft",
        options,
        Box::new(move |cc| {
            let mut app = NoteCraftApp::new(session);
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
            if let Some(p) = &open {
                app.open_path(p);
            }
            Ok(Box::new(Shell { app }))
        }),
    )
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

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
// NoteCraft is a GUI app on Windows in every profile. Diagnostics go to the local log/session
// files rather than opening a console beside the window.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
//! NoteCraft desktop app: a native window over the engine.
//!
//! Usage: `notecraft [PATH] [--notebooks FOLDER] [--open PATH] [--expand-all] [--control PORT]`. The notebooks folder defaults to the
//! `NOTECRAFT_NOTEBOOKS` environment variable, then `Documents/NoteCraft` in the home folder.
//! PATH is a notebook, section group, section or page, relative to the notebooks folder or a
//! full path; anything but a page opens on its first page. A notebook kept elsewhere is opened.

mod applog;
mod telemetry;

use notecraft_engine::Session;
use notecraft_store::Settings;
use notecraft_ui_egui::NoteCraftApp;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::path::PathBuf;

struct Shell {
    windows: notecraft_ui_egui::windows::Host,
    _input_language_watcher: Option<notecraft_ui_egui::input_language::Watcher>,
}

impl eframe::App for Shell {
    /// Puts the clicks, keys and text that control requests queued into the next frame.
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        // Every key press in the app, for debugging (debug log level).
        for event in &raw_input.events {
            if let egui::Event::Key { key, pressed: true, modifiers, .. } = event {
                log::debug!("key press: {key:?} shift={} ctrl={} alt={}", modifiers.shift, modifiers.command, modifiers.alt);
            }
        }
        // Ctrl+V, or Ctrl+Shift+V for the text only (issue #535).
        if let Some(plain) = notecraft_ui_egui::clipboard::take_keyboard_paste(&mut raw_input.events) {
            log::debug!("raw keyboard paste captured before egui clipboard handling (text only: {plain})");
            self.windows.queue_keyboard_paste(raw_input.viewport_id, plain);
        }
        raw_input.events.extend(self.windows.take_events(raw_input.viewport_id));
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.windows.logic(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.windows.show(ui);
    }
}

fn notebooks_dir(args: &[String]) -> PathBuf {
    let configured = notecraft_store::settings::configured_notebooks_dir(&Settings::default_location());
    // The development folder counts only in a debug build; a production install never reads it.
    let local = PathBuf::from("my-notebooks");
    let local = (cfg!(debug_assertions) && local.is_dir()).then_some(local);
    notebooks_dir_from(args, configured, local, notecraft_store::settings::default_notebooks_dir())
}

fn notebooks_dir_from(args: &[String], configured: Option<PathBuf>, local: Option<PathBuf>, fallback: PathBuf) -> PathBuf {
    if let Some(i) = args.iter().position(|a| a == "--notebooks") {
        return args.get(i + 1).map(PathBuf::from).unwrap_or(fallback);
    }
    configured.or(local).unwrap_or(fallback)
}

/// Test/screenshot sandboxes are valid explicit roots but must never become the next
/// production default through `last_notebooks_root`.
fn should_remember_notebooks_root(root: &std::path::Path) -> bool {
    let temp = std::fs::canonicalize(std::env::temp_dir()).unwrap_or_else(|_| std::env::temp_dir());
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    !root.starts_with(temp)
}

/// Locates the generated HTML and CMGFM demo payload installed beside the app.
fn demo_payload(_executable: &std::path::Path) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let installed = _executable.parent().map(|parent| parent.join("demo").join("welcome-to-notecraft"));

    #[cfg(target_os = "linux")]
    let installed: Option<PathBuf> = Some(if let Some(appdir) = std::env::var_os("APPDIR") {
        PathBuf::from(appdir).join("usr/share/notecraft/demo/welcome-to-notecraft")
    } else {
        PathBuf::from("/usr/share/notecraft/demo/welcome-to-notecraft")
    });

    #[cfg(target_os = "macos")]
    let installed = _executable.parent().and_then(|parent| parent.parent()).map(|parent| parent.join("Resources/demo/welcome-to-notecraft"));

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    let installed = _executable.parent().map(|parent| parent.join("demo").join("welcome-to-notecraft"));

    let fallback = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/demo/DemoNotebook");
    Some(choose_demo_payload(installed.as_deref(), &fallback))
}

fn choose_demo_payload(installed: Option<&std::path::Path>, fallback: &std::path::Path) -> PathBuf {
    installed.filter(|path| path.is_dir()).map_or_else(|| fallback.to_path_buf(), std::path::Path::to_path_buf)
}

/// The file the MSI leaves beside the app to ask for the first-use wizard once.
const INSTALL_MARKER: &str = "first-use.pending";

/// Acknowledge the installer marker in per-user settings before trying to remove it. A normal
/// user cannot necessarily delete files in the install directory. A retained marker must not
/// clear completed setup on every launch. A replaced marker can request setup again.
fn take_install_marker(install_dir: &std::path::Path, settings: &Settings) -> Result<bool, String> {
    let marker = install_dir.join(INSTALL_MARKER);
    let metadata = match std::fs::metadata(&marker) {
        Ok(metadata) if metadata.is_file() => metadata,
        Ok(_) => return Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("Could not read the first-use installation marker: {error}")),
    };
    let modified = metadata.modified().map_err(|error| format!("Could not read the installation marker timestamp: {error}"))?;
    let stamp = modified.duration_since(std::time::UNIX_EPOCH).map_err(|error| format!("Invalid installation marker timestamp: {error}"))?;
    let identity = format!("{}:{}:{}", marker.display(), stamp.as_nanos(), metadata.len());
    if settings.get("first_use_install_marker").as_deref() == Some(identity.as_str()) {
        return Ok(false);
    }
    if settings.welcome_notebook_update_prompt() {
        settings.set("privacy_welcome_seen", "").map_err(|error| format!("Could not save first-use preferences: {error}"))?;
    }
    settings.set("first_use_install_marker", &identity).map_err(|error| format!("Could not acknowledge this installation: {error}"))?;
    // Cleanup is optional once this user has acknowledged the marker.
    let _ = std::fs::remove_file(&marker);
    Ok(true)
}

fn flag_value(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

/// Flags that take the word after them.
const VALUE_FLAGS: [&str; 3] = ["--notebooks", "--open", "--control"];

/// What to open: `--open PATH`, or else the first word that is neither a flag nor a flag's
/// value, so `notecraft D:\Notes\Work` and a file manager's "Open with" both work.
fn open_arg(args: &[String]) -> Option<String> {
    if let Some(p) = flag_value(args, "--open") {
        return Some(p);
    }
    let mut words = args.iter();
    while let Some(a) = words.next() {
        if VALUE_FLAGS.contains(&a.as_str()) {
            words.next();
        } else if a == "--new-version" || a == "--set-version" {
            // The version after it is not a page to open.
            if args.iter().skip_while(|x| *x != a).nth(1).is_some_and(|v| v.starts_with(|c: char| c.is_ascii_digit())) {
                words.next();
            }
        } else if !a.starts_with("--") {
            return Some(a.clone());
        }
    }
    None
}

fn icon() -> Option<egui::IconData> {
    let img = match image::load_from_memory(notecraft_ui_egui::LOGO_PNG) {
        Ok(img) => img.into_rgba8(),
        Err(error) => {
            applog::info(&format!("app icon decode failed: {error}"));
            return None;
        }
    };
    let (width, height) = img.dimensions();
    applog::info(&format!("app icon decoded: source=embedded-logo-png size={width}x{height} rgba_bytes={}", img.as_raw().len()));
    Some(egui::IconData { rgba: img.into_raw(), width, height })
}

/// Says why the app cannot start. A release build on Windows has no console, so the message
/// also goes into a box; otherwise the window would just never appear.
fn fail(message: &str) -> ! {
    eprintln!("{message}");
    rfd::MessageDialog::new().set_level(rfd::MessageLevel::Error).set_title("NoteCraft").set_description(message).show();
    std::process::exit(1);
}

/// The NoteCraft copy installed for everyone, when there is one (Windows).
fn installed_copy() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let program_files = std::env::var_os("ProgramFiles").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
        let exe = program_files.join("NoteCraft").join("notecraft.exe");
        exe.is_file().then_some(exe)
    }
    #[cfg(not(windows))]
    None
}

fn main() -> eframe::Result {
    // First, while this is the only thread: on Linux and macOS the offset can only be read
    // safely then. Timestamps are kept in UTC and shown on the user's clock (issue #133); if
    // the offset cannot be read they are shown in UTC, as before.
    let local_offset = time::UtcOffset::current_local_offset();
    if let Ok(offset) = local_offset {
        notecraft_model::time::set_local_offset(offset.whole_seconds());
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The update helper started this copy with --finish-update: it runs the install and the restart,
    // then exits. This must run before any window opens.
    if let Some(result) =
        craft_update::self_update::finish(&notecraft_ui_egui::update_prompt::NOTECRAFT, &std::env::args_os().skip(1).collect::<Vec<_>>())
    {
        match &result {
            Ok(()) => applog::info("update helper: finished"),
            Err(e) => applog::line("ERROR", &format!("update helper: {e}")),
        }
        return Ok(());
    }
    let settings_dir = Settings::default_location().path().parent().map(std::path::Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let telemetry_level =
        Settings::default_location().get("telemetry_level").as_deref().and_then(telemetry::Level::parse).unwrap_or_else(telemetry::Level::for_build);
    applog::init(settings_dir.clone(), telemetry_level == telemetry::Level::Debug);
    let telemetry = (telemetry_level != telemetry::Level::None)
        .then(|| telemetry::Session::start(telemetry::telemetry_root(&settings_dir), telemetry_level).ok())
        .flatten();
    if let Some(session) = &telemetry {
        let _ = Settings::default_location().set("telemetry_session_id", session.id());
        let _ = session.record(telemetry::Level::Usage, "app_start", &[("version", env!("CARGO_PKG_VERSION"))]);
    }
    // The interface records its usage events through this sink. Every event goes to the local log;
    // it also goes to the telemetry session when one exists and its level allows the event.
    let session = telemetry;
    notecraft_ui_egui::usage::install(Box::new(move |level, event, fields| {
        // The log keeps the event name only; field values may name files, so they stay in the telemetry session.
        applog::info(&format!("event {event}"));
        if let Some(session) = &session {
            let level = match level {
                notecraft_ui_egui::usage::Level::Usage => telemetry::Level::Usage,
                notecraft_ui_egui::usage::Level::Warning => telemetry::Level::Warning,
                notecraft_ui_egui::usage::Level::Error => telemetry::Level::Error,
            };
            let _ = session.record(level, event, fields);
        }
    }));
    applog::info(&format!(
        "start: NoteCraft {} exe={:?} args={args:?} settings={:?}",
        env!("CARGO_PKG_VERSION"),
        std::env::current_exe().ok(),
        Settings::default_location().path()
    ));
    match local_offset {
        Ok(offset) => applog::info(&format!("local time is UTC{offset}")),
        Err(error) => applog::info(&format!("local time offset unknown, timestamps are shown in UTC: {error}")),
    }
    if args.iter().any(|a| a == "--version") {
        println!("notecraft {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    // A development run tests an update against the installed copy: it hands `--new-version` to
    // the installed NoteCraft, which then downloads and installs over itself.
    #[allow(clippy::collapsible_if)]
    if cfg!(debug_assertions) && args.iter().any(|a| a == "--new-version" || a == "--set-version") {
        if let Some(installed) = installed_copy() {
            applog::info(&format!("development run: --new-version runs the installed copy {}", installed.display()));
            match std::process::Command::new(&installed).args(&args).spawn() {
                Ok(_) => return Ok(()),
                Err(error) => applog::line("ERROR", &format!("could not start the installed copy: {error}")),
            }
        }
    }
    let root = notebooks_dir(&args);
    applog::info(&format!("notebooks folder: {}", root.display()));
    let executable = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => fail(&format!("Could not locate the generated Welcome notebook payload: {error}")),
    };
    let payload = demo_payload(&executable).unwrap_or_else(|| fail("Could not locate the generated Welcome notebook payload."));
    let session = match Session::new(&root, Settings::default_location()) {
        Ok(s) => s,
        Err(e) => {
            applog::line("ERROR", &format!("cannot open the notebooks folder {}: {e}", root.display()));
            fail(&format!("NoteCraft cannot open the notebooks folder {}: {e}", root.display()));
        }
    };
    // An explicit command-line root becomes the next launch's remembered notebook root.
    // This prevents a demo or another deliberately selected notebook from being replaced
    // by the old default folder on the next start.
    if args.iter().any(|arg| arg == "--notebooks") && should_remember_notebooks_root(&root) {
        let remembered = std::fs::canonicalize(&root).unwrap_or_else(|_| root.clone());
        let _ = session.settings.set("last_notebooks_root", &remembered.to_string_lossy());
    }
    // Reset first-use flags so both current first-run prompts can be reviewed again.
    if args.iter().any(|arg| arg == "--first-use") {
        let _ = session.settings.set("privacy_welcome_seen", "");
        applog::info("First-use flag set: the first-use wizard will show");
    }
    let mut startup_alerts: Vec<(&str, String)> = Vec::new();
    // The installer leaves a marker beside the app. Acknowledge it once for this user.
    if let Some(dir) = executable.parent()
        && let Err(error) = take_install_marker(dir, &session.settings)
    {
        applog::line("ERROR", &error);
        startup_alerts.push(("install-marker", error));
    }

    // NoteCraft draws its own title bar unless the settings say the system's is wanted.
    let own_frame = session.settings.get("window_frame").as_deref() != Some("system");
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([820.0, 520.0])
        .with_title("NoteCraft")
        .with_app_id("com.alientechsoftware.notecraft")
        .with_decorations(!own_frame);
    if let Some(icon) = icon() {
        applog::info(&format!("app icon installed on native viewport: size={}x{} rgba_bytes={}", icon.width, icon.height, icon.rgba.len()));
        viewport = viewport.with_icon(icon);
    } else {
        applog::info("app icon was not installed on native viewport");
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    applog::info(&format!("session open ({} tree nodes); starting the window", session.tree().len()));
    let open = open_arg(&args);
    let expand = args.iter().any(|a| a == "--expand-all");
    let control_port: Option<u16> =
        flag_value(&args, "--control").or_else(|| std::env::var("NOTECRAFT_CONTROL_PORT").ok()).and_then(|p| p.parse().ok());
    let key_dir = Settings::default_location().path().parent().map(std::path::Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    // The clipboard is opened once, here. If it cannot be, the app still runs without it, and the
    // window says so.
    let clipboard: Box<dyn notecraft_ui_egui::clipboard::Board> = match notecraft_ui_egui::clipboard::open() {
        Ok(board) => board,
        Err(e) => {
            let message = format!("The clipboard could not be opened, so copy and paste do not work this session: {e}");
            eprintln!("{message}");
            startup_alerts.push(("clipboard-open", message));
            Box::new(notecraft_ui_egui::clipboard::Unavailable)
        }
    };
    let result = eframe::run_native(
        "NoteCraft",
        options,
        Box::new(move |cc| {
            let setup_payload = payload.clone();
            let mut app = NoteCraftApp::new(session)
                .with_welcome_notebook_setup(move |notebooks_root, format| {
                    notecraft_setup::install_edition(&setup_payload, notebooks_root, format).map_err(|error| error.to_string())
                })
                .with_clipboard(clipboard);
            for (key, message) in startup_alerts.drain(..) {
                app.alert(key, "NoteCraft", &message);
            }
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
            // `--new-version` shows a newer version; `--new-version 0.0.14` names it. Nothing is downloaded.
            if let Some(i) = args.iter().position(|a| a == "--new-version") {
                let named = args.get(i + 1).filter(|v| v.starts_with(|c: char| c.is_ascii_digit()));
                app.skip_first_use_wizard();
                app.simulate_new_version(named.map_or(notecraft_ui_egui::app::SIMULATED_NEW_VERSION, String::as_str));
            }
            // `--set-version 0.0.8` runs the real check as an installed copy of that version, for this
            // run only. Nothing is saved, so the next start is the real version again.
            if let Some(folder) = args.iter().position(|a| a == "--update-from").and_then(|i| args.get(i + 1)) {
                {
                    let running = args
                        .iter()
                        .position(|a| a == "--set-version")
                        .and_then(|j| args.get(j + 1))
                        .cloned()
                        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
                    app.skip_first_use_wizard();
                    app.start_local_update_check(&running, std::path::Path::new(folder));
                }
            }
            // A local update folder replaces the GitHub check, so --set-version does not start its own.
            // An installed copy checks GitHub at every start, as the update card promises. A
            // development run checks only when --set-version asks for it.
            // Only an installed copy checks by itself. A build tree (including a release build run
            // with cargo) is not installed, so the updater's own test decides it.
            let installed_start = craft_update::self_update::current(&notecraft_ui_egui::update_prompt::NOTECRAFT).is_some()
                && app.update_check_at_start()
                && app.first_use_done()
                && !args.iter().any(|a| a == "--update-from" || a == "--set-version" || a == "--new-version");
            if installed_start {
                app.start_update_check(env!("CARGO_PKG_VERSION"));
            }
            if let Some(running) = args
                .iter()
                .position(|a| a == "--set-version")
                .filter(|_| !args.iter().any(|a| a == "--update-from"))
                .and_then(|i| args.get(i + 1))
                .filter(|v| v.starts_with(|c: char| c.is_ascii_digit()))
            {
                app.skip_first_use_wizard();
                app.start_update_check(running);
            }
            if expand {
                app.expand_all();
            }
            if let Some(p) = open.as_ref() {
                app.open_path(p);
            }
            applog::info("window ready");
            Ok(Box::new(Shell { windows: notecraft_ui_egui::windows::Host::new(app), _input_language_watcher: input_language_watcher }))
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
    fn an_installer_marker_asks_for_the_first_use_wizard_once() {
        let t = notecraft_store::testing::TempDir::new("install-marker");
        let settings = Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("seen before install");
        std::fs::write(t.path().join(INSTALL_MARKER), "").expect("installer marker");

        assert!(take_install_marker(t.path(), &settings).expect("acknowledge marker"));
        assert_eq!(settings.get("privacy_welcome_seen").as_deref(), Some(""), "the wizard shows again");
        assert!(!t.path().join(INSTALL_MARKER).exists(), "the marker is used up");
        assert!(!take_install_marker(t.path(), &settings).expect("read marker"), "a second launch does not ask again");
    }

    #[test]
    fn no_installer_marker_is_not_consumed() {
        let t = notecraft_store::testing::TempDir::new("install-marker-absent");
        let settings = Settings::at(t.path().join("settings.toml"));
        assert!(!take_install_marker(t.path(), &settings).expect("read marker"));
        assert!(!t.path().join(INSTALL_MARKER).exists());
    }

    #[test]
    fn welcome_update_opt_out_survives_new_install_markers() {
        let t = notecraft_store::testing::TempDir::new("welcome-update-opt-out");
        let marker = t.path().join(INSTALL_MARKER);
        let settings = Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("finish setup");
        settings.set_welcome_notebook_update_prompt(false).expect("opt out");

        let settings = Settings::at(settings.path());
        for contents in ["first upgrade", "next upgrade"] {
            std::fs::write(&marker, contents).expect("installer marker");
            assert!(take_install_marker(t.path(), &settings).expect("acknowledge marker"));
            assert!(!settings.first_use_pending(), "completed setup stays completed");
        }

        settings.set_welcome_notebook_update_prompt(true).expect("opt back in");
        std::fs::write(&marker, "another upgrade").expect("installer marker");
        assert!(take_install_marker(t.path(), &settings).expect("acknowledge marker"));
        assert!(settings.first_use_pending(), "a later upgrade prompts after opting back in");
    }

    #[test]
    fn welcome_update_opt_out_keeps_pending_first_use() {
        let t = notecraft_store::testing::TempDir::new("welcome-update-first-use");
        let settings = Settings::at(t.path().join("settings.toml"));
        settings.set_welcome_notebook_update_prompt(false).expect("opt out");
        std::fs::write(t.path().join(INSTALL_MARKER), "").expect("installer marker");

        assert!(take_install_marker(t.path(), &settings).expect("acknowledge marker"));
        assert!(settings.first_use_pending(), "first-use setup is still pending");
    }

    #[test]
    fn an_already_acknowledged_install_marker_does_not_reset_preferences() {
        let t = notecraft_store::testing::TempDir::new("install-marker-retained");
        let marker = t.path().join(INSTALL_MARKER);
        let settings = Settings::at(t.path().join("settings.toml"));
        std::fs::write(&marker, "installed").expect("installer marker");
        let modified = std::fs::metadata(&marker).expect("metadata").modified().expect("modified");
        assert!(take_install_marker(t.path(), &settings).expect("acknowledge marker"));
        settings.set("privacy_welcome_seen", "true").expect("finish setup");

        // An installer can restore the same marker during repair. Its identity is unchanged.
        std::fs::write(&marker, "installed").expect("restore installer marker");
        std::fs::File::options().write(true).open(&marker).expect("open marker").set_modified(modified).expect("restore timestamp");
        assert!(!take_install_marker(t.path(), &settings).expect("read marker"), "the same installation is acknowledged per user");
        assert_eq!(settings.get("privacy_welcome_seen").as_deref(), Some("true"), "completed preferences stay completed");
    }

    #[cfg(windows)]
    #[test]
    fn a_locked_install_marker_only_requests_setup_once() {
        use std::os::windows::fs::OpenOptionsExt;

        let t = notecraft_store::testing::TempDir::new("install-marker-locked");
        let marker = t.path().join(INSTALL_MARKER);
        let settings = Settings::at(t.path().join("settings.toml"));
        std::fs::write(&marker, "installed").expect("installer marker");
        // Permit reads and writes but deny deletion, as an install directory can do.
        let held = std::fs::OpenOptions::new().read(true).share_mode(3).open(&marker).expect("hold marker without delete sharing");
        let first = take_install_marker(t.path(), &settings).expect("acknowledge marker");
        assert!(marker.is_file(), "the real filesystem refuses marker cleanup");
        settings.set("privacy_welcome_seen", "true").expect("finish setup");
        let second = take_install_marker(t.path(), &settings).expect("read marker");
        let seen = settings.get("privacy_welcome_seen");
        drop(held);
        assert!(first, "first launch requests setup");
        assert!(!second, "a marker that cannot be deleted does not reset setup on every launch");
        assert_eq!(seen.as_deref(), Some("true"));
    }

    #[test]
    fn a_new_install_marker_and_a_new_user_each_request_setup() {
        let t = notecraft_store::testing::TempDir::new("install-marker-new-cycle");
        let marker = t.path().join(INSTALL_MARKER);
        let first = Settings::at(t.path().join("first-user.toml"));
        let second = Settings::at(t.path().join("second-user.toml"));
        std::fs::write(&marker, "installed").expect("installer marker");
        let modified = std::fs::metadata(&marker).expect("metadata").modified().expect("modified");
        assert!(take_install_marker(t.path(), &first).expect("first user"));
        first.set("privacy_welcome_seen", "true").expect("finish setup");
        std::fs::write(&marker, "installed").expect("restore marker");
        std::fs::File::options().write(true).open(&marker).expect("marker").set_modified(modified).expect("original timestamp");
        assert!(take_install_marker(t.path(), &second).expect("second user"), "acknowledgement is per user");
        assert_eq!(first.get("privacy_welcome_seen").as_deref(), Some("true"));
        std::fs::write(&marker, "installed").expect("new installer marker");
        std::fs::File::options()
            .write(true)
            .open(&marker)
            .expect("marker")
            .set_modified(modified + std::time::Duration::from_secs(2))
            .expect("new installation timestamp");
        assert!(take_install_marker(t.path(), &first).expect("new installation"));
        assert_eq!(first.get("privacy_welcome_seen").as_deref(), Some(""));
    }

    #[test]
    fn installer_marker_stays_when_settings_cannot_be_saved() {
        let t = notecraft_store::testing::TempDir::new("install-marker-settings-failure");
        let marker = t.path().join(INSTALL_MARKER);
        std::fs::write(&marker, "pending").expect("write marker");
        let settings_parent = t.path().join("settings-parent");
        std::fs::write(&settings_parent, "a file where the settings directory should be").expect("block settings directory");
        let settings = Settings::at(settings_parent.join("settings.toml"));

        assert!(take_install_marker(t.path(), &settings).is_err(), "a save failure is actionable at startup");
        assert!(marker.is_file(), "the next launch must retry the first-use wizard");
    }

    #[test]
    fn the_notebooks_flag_wins() {
        let args = vec!["--notebooks".to_string(), "somewhere".to_string()];
        assert_eq!(notebooks_dir(&args), PathBuf::from("somewhere"));
    }

    #[test]
    fn notebook_root_candidates_follow_cli_settings_local_default_precedence() {
        let configured = PathBuf::from("configured-notebooks");
        let local = PathBuf::from("my-notebooks");
        let fallback = PathBuf::from("Documents/NoteCraft");
        let args = Vec::new();

        assert_eq!(notebooks_dir_from(&args, Some(configured.clone()), Some(local.clone()), fallback.clone()), configured);
        assert_eq!(notebooks_dir_from(&args, None, Some(local.clone()), fallback.clone()), local);
        assert_eq!(notebooks_dir_from(&args, None, None, fallback.clone()), fallback);
        assert_eq!(notebooks_dir_from(&["--notebooks".into(), "cli-root".into()], None, None, PathBuf::from("default")), PathBuf::from("cli-root"));
    }

    #[test]
    fn notebook_flag_without_a_value_uses_the_default_not_a_saved_root() {
        let args = vec!["--notebooks".to_string()];
        assert_eq!(
            notebooks_dir_from(&args, Some(PathBuf::from("saved")), Some(PathBuf::from("local")), PathBuf::from("default")),
            PathBuf::from("default")
        );
    }

    #[test]
    fn a_trailing_flag_without_a_value_falls_back_to_the_default() {
        let args = vec!["--notebooks".to_string()];
        assert_eq!(notebooks_dir(&args), notecraft_store::settings::default_notebooks_dir());
    }

    #[test]
    fn a_version_after_new_version_is_not_a_page_to_open() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(open_arg(&a(&["--new-version", "0.0.14"])), None);
        assert_eq!(open_arg(&a(&["--new-version", "0.0.14", "Notes/Work"])), Some("Notes/Work".to_string()));
        assert_eq!(open_arg(&a(&["--set-version", "0.0.8"])), None);
    }

    #[test]
    fn a_temp_notebooks_root_is_not_persisted_as_the_production_default() {
        assert!(!should_remember_notebooks_root(&std::env::temp_dir()));
        assert!(should_remember_notebooks_root(std::path::Path::new("Documents/NoteCraft")));
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn a_windows_saved_path_is_not_used_on_linux() {
        let settings = Settings::at(std::env::temp_dir().join("notecraft-windows-saved-path.toml"));
        settings.set("last_notebooks_root", r"C:\Notes").expect("save Windows path");
        assert_eq!(notecraft_store::settings::configured_notebooks_dir(&settings), None);
    }

    #[test]
    fn the_path_to_open_is_the_flag_or_the_first_plain_word() {
        let a = |w: &[&str]| w.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(open_arg(&a(&["--open", "nb/page"])).as_deref(), Some("nb/page"));
        assert_eq!(open_arg(&a(&["D:\\Notes\\Work"])).as_deref(), Some("D:\\Notes\\Work"));
        // A flag's value is not the path, and a switch is skipped.
        assert_eq!(open_arg(&a(&["--notebooks", "root", "--expand-all", "nb"])).as_deref(), Some("nb"));
        assert_eq!(open_arg(&a(&["--control", "0", "--notebooks", "root"])), None);
        assert_eq!(open_arg(&a(&[])), None);
    }

    #[test]
    fn the_icon_decodes() {
        let i = icon().expect("icon");
        assert_eq!((i.width, i.height), (256, 256));
        assert_eq!(i.rgba.len(), 256 * 256 * 4);
    }

    #[test]
    fn installed_demo_payload_wins_and_missing_install_uses_the_source_copy() {
        let t = notecraft_store::testing::TempDir::new("demo-payload");
        let installed = t.path().join("installed");
        let fallback = t.path().join("source");
        std::fs::create_dir_all(&installed).expect("create installed payload");
        std::fs::create_dir_all(&fallback).expect("create source payload");

        assert_eq!(choose_demo_payload(Some(&installed), &fallback), installed);
        let missing = t.path().join("missing");
        assert_eq!(choose_demo_payload(Some(&missing), &fallback), fallback);
        assert_eq!(choose_demo_payload(None, &missing), missing);
    }

    #[test]
    fn explicit_open_path_wins_over_an_earlier_positional_path() {
        let args = vec!["positional".to_string(), "--open".to_string(), "explicit".to_string()];
        assert_eq!(open_arg(&args).as_deref(), Some("explicit"));
    }

    #[test]
    fn a_missing_open_value_falls_back_to_the_positional_path() {
        let args = vec!["positional".to_string(), "--open".to_string()];
        assert_eq!(open_arg(&args).as_deref(), Some("positional"));
    }
}

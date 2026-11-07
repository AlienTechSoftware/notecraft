#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use notecraft_setup::{SetupOutcome, refresh_demo};
use notecraft_store::{Settings, settings::default_notebooks_dir};
use std::path::{Path, PathBuf};
#[cfg(not(target_os = "macos"))]
use std::process::Command;

fn locate_payload(executable: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let candidate = executable.parent()?.join("demo").join("welcome-to-notecraft");

    #[cfg(target_os = "linux")]
    let candidate = if let Some(appdir) = std::env::var_os("APPDIR") {
        PathBuf::from(appdir).join("usr/share/notecraft/demo/welcome-to-notecraft")
    } else {
        executable.parent()?.join("../share/notecraft/demo/welcome-to-notecraft")
    };

    #[cfg(target_os = "macos")]
    let candidate = notecraft_setup::macos_dmg_payload(executable)?;

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    let candidate = executable.parent()?.join("demo").join("welcome-to-notecraft");

    candidate.is_dir().then_some(candidate)
}

#[cfg(not(target_os = "macos"))]
fn sibling_app(executable: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) { "notecraft.exe" } else { "notecraft" };
    let path = executable.parent()?.join(name);
    path.is_file().then_some(path)
}

fn main() {
    let executable = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => fail(&format!("Could not locate the NoteCraft setup tool: {error}")),
    };

    #[cfg(target_os = "macos")]
    {
        install_from_dmg(&executable);
    }

    #[cfg(not(target_os = "macos"))]
    {
        launch_desktop_app(&executable);
    }
}

#[cfg(not(target_os = "macos"))]
fn launch_desktop_app(executable: &Path) {
    let app = match sibling_app(executable) {
        Some(path) => path,
        None => fail("The NoteCraft app is missing beside its setup tool. Reinstall NoteCraft and try again."),
    };
    let settings = Settings::default_location();
    let payload = locate_payload(executable);
    let payload = payload.as_deref().unwrap_or_else(|| fail("The generated Welcome notebook is missing from this installation."));
    let notebooks = notecraft_store::settings::configured_notebooks_dir(&settings).unwrap_or_else(default_notebooks_dir);
    let outcome = match refresh_demo(payload, &notebooks) {
        Ok(outcome) => outcome,
        Err(error) => fail(&format!("The User Assistance notebook could not be installed: {error}")),
    };

    let mut command = Command::new(app);
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        let SetupOutcome::Installed(path) = outcome;
        command.arg("--open").arg(path);
    } else {
        command.args(args);
    }

    match command.status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(error) => fail(&format!("NoteCraft could not be opened: {error}")),
    }
}

#[cfg(target_os = "macos")]
fn install_from_dmg(executable: &Path) {
    let settings = Settings::default_location();
    let notebooks = notecraft_store::settings::configured_notebooks_dir(&settings).unwrap_or_else(default_notebooks_dir);
    let payload = locate_payload(executable);
    let Some(payload) = payload else {
        show_notice("The Welcome notebook is not included in this NoteCraft disk image. NoteCraft is ready to use.");
        return;
    };
    match refresh_demo(&payload, &notebooks) {
        Ok(SetupOutcome::Installed(path)) => {
            show_notice(&format!("The Welcome notebook was refreshed in {}. You can now open NoteCraft from Applications.", path.display()))
        }
        Err(error) => fail(&format!("The Welcome notebook could not be installed: {error}")),
    }
}

#[cfg(target_os = "macos")]
fn show_notice(message: &str) {
    let _ = rfd::MessageDialog::new().set_level(rfd::MessageLevel::Info).set_title("NoteCraft Setup").set_description(message).show();
}

fn fail(message: &str) -> ! {
    let _ = rfd::MessageDialog::new().set_title("NoteCraft setup").set_description(message).show();
    eprintln!("{message}");
    std::process::exit(1)
}

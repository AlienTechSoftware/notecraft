//! The update prompt, from the shared `craft-update` crate, in NoteCraft's own look: our icon,
//! our accent colour and our words. Only the prompt is shared; installing still follows our own
//! update code.

use std::sync::Arc;

use crate::update_card::updater::{CheckSource, Updater};
use craft_update::App;
use craft_update::release::Release;

/// NoteCraft as the shared prompt names it. Its bundle identifier is the one the macOS package uses.
pub static NOTECRAFT: App = App {
    id: "notecraft",
    name: "NoteCraft",
    tagline: "A clean-room OneNote recreation for local files.",
    repo: "AlienTechSoftware/notecraft",
    former_ids: &[],
    former_names: &[],
    icon_png: Some(include_bytes!("../../../assets/brand/notecraft-128.png")),
    accent: Some([0x7a, 0x45, 0xb8]),
    developer: "NoteCraft",
};

/// The prompt's words, in the interface language. The crate looks each English text up here.
pub fn translate(text: &str, args: &[(&str, &str)]) -> String {
    let id = match text {
        "{app} {version} is available with the latest features and bug fixes." => "ui.update_prompt.available",
        "by {developer}" => "ui.update_prompt.by_developer",
        "v{from} to {to}" => "ui.update_prompt.version_change",
        "What's new" => "ui.update_prompt.whats_new",
        "Download" => "ui.update_prompt.download",
        "{app} {version} is the newest version available." => "ui.update_prompt.newest_available",
        "Later" => "ui.update_prompt.later",
        "Dismiss" => "ui.update_prompt.dismiss",
        "All releases are listed at {url}" => "ui.update_prompt.all_releases",
        "Update" => "ui.update_prompt.update",
        "Restart to Update" => "ui.update_prompt.restart_to_update",
        "Update and Restart" => "ui.update_prompt.update_and_restart",
        "Downloading and installing update…" => "ui.update_prompt.downloading",
        "Close" => "ui.update_prompt.close",
        _ => return craft_update::translate::english(text, args),
    };
    crate::i18n::format_message_id(id, args)
}

/// The repository the prompt and its downloads come from. NoteCraft's own releases only.
pub const REPO: &str = "AlienTechSoftware/notecraft";

/// The real check: asks GitHub for the latest release, and offers it when it is newer than
/// `running`. `--set-version` runs it as an older copy, for one run; nothing is saved.
pub fn check_latest(running: &str) -> Updater {
    let running_owned = running.to_string();
    // Detecting the installation probes the system (package managers on Linux), so it runs on the
    // check thread. The window reads the answer when the card opens.
    let installable = Arc::new(std::sync::OnceLock::new());
    let decided = installable.clone();
    let source: CheckSource = Arc::new(move || {
        let is_self_updating = craft_update::self_update::is_self_updating(&NOTECRAFT);
        log::info!("update: installable, decided on the check thread: {is_self_updating}");
        let _ = decided.set(is_self_updating);
        let http = craft_update::Http::new(&format!("NoteCraft/{running_owned}"))?;
        let answer = craft_update::check_for_update(&NOTECRAFT, &http, &running_owned);
        log::info!("update: latest release check finished: {:?}", answer.as_ref().map(|r| r.as_ref().map(|x| x.tag.clone())));
        answer
    });
    Updater::new(&NOTECRAFT, running).with_translator(translate).with_installable_cell(installable).with_source(source)
}

/// A prompt offering release `version` of NoteCraft, read from its GitHub release. The first
/// frame that calls `check_at_start` asks GitHub for that tag; the prompt's download and install
/// are then the shared flow. `--new-version 0.0.14` uses it.
pub fn offer_named(version: &str) -> Updater {
    let tag = format!("v{version}");
    let named = version.to_string();
    log::info!("update: install for this copy: {:?}", craft_update::self_update::current(&NOTECRAFT));
    let source: CheckSource = Arc::new(move || {
        let http = craft_update::Http::new(&format!("NoteCraft/{}", crate::backstage::VERSION))?;
        let url = format!("https://api.github.com/repos/{REPO}/releases/tags/{tag}");
        log::info!("update: asking GitHub for release {tag}");
        match http.get_api(&url, None)? {
            craft_update::http::Fetched::Body { text, .. } => {
                log::info!("update: GitHub has release {tag}; self-updating: {}", craft_update::self_update::is_self_updating(&NOTECRAFT));
                craft_update::release::parse_github_release(&text, REPO).map(Some)
            }
            craft_update::http::Fetched::NotFound => {
                log::warn!("update: GitHub has no release {tag}");
                Err(format!("NoteCraft {named} has no release on GitHub"))
            }
            _ => Err("GitHub did not answer the release request".to_string()),
        }
    });
    let installable = craft_update::self_update::is_self_updating(&NOTECRAFT);
    Updater::new(&NOTECRAFT, crate::backstage::VERSION).with_translator(translate).with_installable(installable).with_source(source)
}

/// A test source: the release is read from `folder`, which holds `SHA256SUMS.txt` and the
/// installer. The installer is still checked for its signature and checksum, as a download is.
/// Used only with `--update-from`, for testing an update before a release is published.
pub fn check_from_folder(running: &str, folder: &std::path::Path) -> Updater {
    let folder = folder.to_path_buf();
    let source: CheckSource = Arc::new(move || {
        let sums_text =
            std::fs::read_to_string(folder.join("SHA256SUMS.txt")).map_err(|e| format!("no SHA256SUMS.txt in {} ({e})", folder.display()))?;
        let sums = craft_update::release::parse_sha256sums(&sums_text);
        let (msi_name, msi_hash) = sums
            .iter()
            .find(|(_, name)| name.ends_with("-windows-x64.msi"))
            .map(|(hash, name)| (name.clone(), hash.clone()))
            .ok_or_else(|| "SHA256SUMS.txt lists no windows-x64 installer".to_string())?;
        let version_text = msi_name.trim_start_matches("notecraft-").split('-').next().unwrap_or("").to_string();
        let version = craft_update::version::parse(&version_text);
        let url = |name: &str| format!("file:///{}", folder.join(name).display().to_string().replace(char::from(92u8), "/"));
        let size = std::fs::metadata(folder.join(&msi_name)).map(|m| m.len()).unwrap_or(0);
        Ok(Some(Release {
            tag: format!("v{version_text}"),
            version,
            name: format!("NoteCraft {version_text}"),
            html_url: crate::backstage::LATEST_RELEASE.to_string(),
            assets: vec![
                craft_update::release::Asset { name: msi_name.clone(), size, url: url(&msi_name), sha256: Some(msi_hash) },
                craft_update::release::Asset {
                    name: "SHA256SUMS.txt".to_string(),
                    size: sums_text.len() as u64,
                    url: url("SHA256SUMS.txt"),
                    sha256: None,
                },
            ],
            ..Release::default()
        }))
    });
    let installable = craft_update::self_update::is_self_updating(&NOTECRAFT);
    Updater::new(&NOTECRAFT, running).with_translator(translate).with_installable(installable).with_source(source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_update::release::Release;

    /// The prompt opens for a newer release, with no window: egui runs its frames headlessly.
    #[test]
    fn a_newer_release_opens_the_prompt_without_a_window() {
        let pins = notecraft_store::testing::TempDir::new("update-prompt-pins");
        let source: CheckSource = Arc::new(|| {
            Ok(Some(Release {
                tag: "v0.0.14".to_string(),
                version: craft_update::version::parse("0.0.14"),
                name: "NoteCraft 0.0.14".to_string(),
                html_url: crate::backstage::LATEST_RELEASE.to_string(),
                ..Release::default()
            }))
        });
        let mut updater =
            Updater::new(&NOTECRAFT, "0.0.9").with_translator(translate).with_source(source).with_pins_path(pins.path().join("pins.json"));
        let ctx = egui::Context::default();
        for _ in 0..500 {
            ctx.begin_pass(egui::RawInput::default());
            updater.check_at_start(&ctx);
            updater.tick(&ctx);
            let _ = updater.show(&ctx);
            let mut output = ctx.end_pass();
            output.textures_delta.clear();
            if updater.prompt_open() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(updater.prompt_open(), "a newer release opens the prompt");
    }
}

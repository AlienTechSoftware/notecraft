//! The update card and its state machine, in NoteCraft's own UI layer. The shared logic
//! (checks, downloads, installs) is in the `craft-update` crate; this module only draws it.
pub mod dialog;
pub mod macos;
pub mod updater;

#[cfg(test)]
mod tests {
    use super::updater::{CheckSource, Updater};
    use craft_update::release::Release;
    use std::sync::Arc;

    static APP: craft_update::App = craft_update::App::new("notecraft", "NoteCraft", "test", "AlienTechSoftware/notecraft");

    /// The card's button follows the decision made once, not a fresh check while drawing: with
    /// `installable` set, the prompt is installable even when the system would say no.
    #[test]
    fn the_card_uses_the_decision_made_once() {
        let release = Release {
            tag: "v9.9.9".to_string(),
            version: craft_update::version::parse("9.9.9"),
            html_url: "https://github.com/AlienTechSoftware/notecraft/releases/tag/v9.9.9".to_string(),
            ..Release::default()
        };
        let source: CheckSource = Arc::new(move || Ok(Some(release.clone())));
        let pins = std::env::temp_dir().join("notecraft-card-test-no-pins").join("versions.json");
        let mut updater = Updater::new(&APP, "0.0.1").with_source(source).with_installable(true).with_pins_path(pins);
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
        assert!(updater.prompt.as_ref().is_some_and(|p| p.is_self_updating), "the decision made once decides the button");
    }

    /// B5: a newer release with no installer for this computer offers Download, not Update.
    #[test]
    fn a_release_with_no_installer_here_offers_download() {
        let release = Release {
            tag: "v9.9.9".to_string(),
            version: craft_update::version::parse("9.9.9"),
            html_url: "https://github.com/AlienTechSoftware/notecraft/releases/tag/v9.9.9".to_string(),
            ..Release::default()
        };
        let source: CheckSource = Arc::new(move || Ok(Some(release.clone())));
        let pins = std::env::temp_dir().join("notecraft-card-test-no-installer").join("versions.json");
        let mut updater = Updater::new(&APP, "0.0.1").with_source(source).with_installable(false).with_pins_path(pins);
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
        assert!(updater.prompt_open(), "a newer release still opens the card");
        assert!(updater.prompt.as_ref().is_some_and(|p| !p.is_self_updating), "no installer here means Download, not Update");
    }
}

#[cfg(test)]
mod macos_route_tests {
    use super::macos::manual_update_page;
    use super::updater::Updater;

    static APP: craft_update::App = craft_update::App::new("notecraft", "NoteCraft", "test", "AlienTechSoftware/notecraft");

    /// On macOS, Update and Restart opens the GitHub releases page instead of installing, and on
    /// other systems nothing changes. Pinned until NoteCraft is signed for macOS.
    #[test]
    fn macos_update_opens_the_releases_page_and_other_systems_do_not() {
        let pins = std::env::temp_dir().join("notecraft-macos-route-no-pins").join("versions.json");
        let mut updater = Updater::new(&APP, "0.0.1").with_pins_path(pins);
        assert!(!updater.manual_on_this_os(false), "other systems keep the in-place update");
        assert!(updater.pending_open.is_none());
        assert!(updater.manual_on_this_os(true), "macOS takes the manual route");
        assert_eq!(updater.pending_open.as_deref(), Some(manual_update_page()));
    }
}

/// Version and download rules for the update card, pinned headlessly. These are the scenarios
/// in the private update test plan (B4, C1, C2, E4, E5).
#[cfg(test)]
mod version_rule_tests {
    use super::updater::{CheckSource, Updater};
    use craft_update::release::Release;
    use std::sync::Arc;

    static APP: craft_update::App = craft_update::App::new("notecraft", "NoteCraft", "test", "AlienTechSoftware/notecraft");

    /// Runs one check against a release `version`, from an installed copy at `running`, and says
    /// whether the card opened an offer.
    fn offers(running: &str, version: &str) -> bool {
        let release = Release {
            tag: format!("v{version}"),
            version: craft_update::version::parse(version),
            html_url: "https://github.com/AlienTechSoftware/notecraft/releases/tag/v".to_string() + version,
            ..Release::default()
        };
        let source: CheckSource = Arc::new(move || Ok(Some(release.clone())));
        let pins = std::env::temp_dir().join("notecraft-version-rule-no-pins").join("versions.json");
        let mut updater = Updater::new(&APP, running).with_source(source).with_installable(true).with_pins_path(pins);
        let ctx = egui::Context::default();
        for _ in 0..200 {
            ctx.begin_pass(egui::RawInput::default());
            updater.check_at_start(&ctx);
            updater.tick(&ctx);
            let _ = updater.show(&ctx);
            let mut output = ctx.end_pass();
            output.textures_delta.clear();
            if updater.prompt_open() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        false
    }

    /// B4: the same version installs nothing and is not offered.
    #[test]
    fn the_same_version_is_not_offered() {
        assert!(!offers("0.0.9", "0.0.9"), "an equal release installs nothing");
    }

    /// C1: an older release is not offered.
    #[test]
    fn an_older_release_is_not_offered() {
        assert!(!offers("0.0.9", "0.0.8"), "an older release is never offered");
    }

    /// C2 and the positive case: a newer release is offered.
    #[test]
    fn a_newer_release_is_offered() {
        assert!(offers("0.0.9", "0.0.10"), "a newer release opens the card");
    }

    /// E4: a file that does not match its published checksum is refused and removed.
    #[test]
    fn a_checksum_mismatch_is_refused() {
        let dir = std::env::temp_dir().join("notecraft-checksum-test");
        let _ = std::fs::create_dir_all(&dir);
        let src = dir.join("source.msi");
        std::fs::write(&src, b"not the file it claims to be").expect("write the test file");
        let dest = dir.join("dest.msi");
        let url = format!("file:///{}", src.display().to_string().replace(char::from(92u8), "/"));
        let wrong = "0".repeat(64);
        let http = craft_update::Http::new("NoteCraft-test/0").expect("http");
        let stop = craft_update::Stop::default();
        let result = http.download(&url, &dest, 28, &wrong, &stop, &mut |_| {});
        assert!(result.is_err(), "a mismatched checksum is refused");
        assert!(!dest.exists(), "the refused file is not kept");
    }

    /// E5: a release with no checksum is refused and nothing is downloaded.
    #[test]
    fn a_missing_checksum_is_refused() {
        let dir = std::env::temp_dir().join("notecraft-checksum-test-missing");
        let _ = std::fs::create_dir_all(&dir);
        let src = dir.join("source.msi");
        std::fs::write(&src, b"file").expect("write the test file");
        let dest = dir.join("dest.msi");
        let url = format!("file:///{}", src.display().to_string().replace(char::from(92u8), "/"));
        let http = craft_update::Http::new("NoteCraft-test/0").expect("http");
        let stop = craft_update::Stop::default();
        let result = http.download(&url, &dest, 4, "", &stop, &mut |_| {});
        assert!(result.is_err(), "a release with no checksum is refused");
        assert!(!dest.exists());
    }
}

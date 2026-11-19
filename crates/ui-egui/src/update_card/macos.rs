//! macOS updates, pinned for now.
//!
//! Until NoteCraft is signed with an Apple Developer ID (paid, and notarised), the updater cannot
//! install an update on macOS, because it refuses unsigned apps. So macOS users are directed to
//! the releases page on GitHub, and the card says so. This is deliberate. When signing is in
//! place, remove this file's pin and let the shared updater install, as it does on Windows.

/// The alert shown on macOS in place of an in-place update.
pub const MANUAL_UPDATE_NOTE: &str = "This version of NoteCraft on macOS is installed by hand. Open the releases page, download the new disk image, and drag NoteCraft to Applications.";

/// The page macOS users are sent to: NoteCraft's releases on GitHub.
pub fn manual_update_page() -> &'static str {
    crate::backstage::LATEST_RELEASE
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the macOS behaviour: no in-place install, a clear alert, and the GitHub releases page.
    /// Until NoteCraft is signed for macOS, this must stay true.
    #[test]
    fn macos_is_directed_to_the_github_releases_page() {
        assert_eq!(manual_update_page(), "https://github.com/AlienTechSoftware/notecraft/releases/latest");
        assert!(MANUAL_UPDATE_NOTE.contains("by hand"), "the alert says the update is manual");
        assert!(MANUAL_UPDATE_NOTE.contains("releases page"), "the alert points at the releases page");
    }
}

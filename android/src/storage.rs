//! Where NoteCraft keeps its files on Android.
//!
//! Android gives every app two folders of its own and no others without a permission:
//! internal storage (`Context.getFilesDir()`), private to the app, and app-specific external
//! storage (`Context.getExternalFilesDir(null)`), which the user can reach over USB and from
//! file managers. Both are removed when the app is uninstalled, as the installation contract
//! (`packaging/installation artifacts.md`) records.
//!
//! - Settings, logs and the unpacked Welcome payload are application files: internal storage.
//! - Notebooks are the user's: `NoteCraft` in external storage, so they can be copied off the
//!   device. When the device has no external storage, internal storage is used instead.

use std::path::{Path, PathBuf};

/// The folder name the notebooks live in, as on the desktop (`Documents/NoteCraft`).
pub const NOTEBOOKS_FOLDER: &str = "NoteCraft";

/// The app's folders on one device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    /// `settings.toml`, the same file the desktop keeps in its config folder.
    pub settings: PathBuf,
    /// The folder the log and telemetry live in.
    pub app_data: PathBuf,
    /// The unpacked Welcome notebook payload, `html/` and `cmgfm/` inside.
    pub payload: PathBuf,
    /// The notebooks root.
    pub notebooks: PathBuf,
}

/// The layout for the folders Android reported. `None` when it reported no internal storage,
/// which a running app always has; there is then nowhere to keep the settings.
pub fn layout(internal: Option<&Path>, external: Option<&Path>) -> Option<Layout> {
    let internal = internal?;
    let notebooks = external.unwrap_or(internal).join(NOTEBOOKS_FOLDER);
    Some(Layout {
        settings: internal.join("settings.toml"),
        app_data: internal.to_path_buf(),
        payload: internal.join("demo").join(crate::welcome::NOTEBOOK_NAME),
        notebooks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notebooks_go_to_external_storage_and_settings_stay_private() {
        let internal = Path::new("/data/user/0/com.alientechsoftware.notecraft/files");
        let external = Path::new("/storage/emulated/0/Android/data/com.alientechsoftware.notecraft/files");
        let layout = layout(Some(internal), Some(external)).expect("layout");
        assert_eq!(layout.notebooks, external.join("NoteCraft"));
        assert_eq!(layout.settings, internal.join("settings.toml"));
        assert_eq!(layout.payload, internal.join("demo").join("welcome-to-notecraft"));
        assert!(layout.app_data.starts_with(internal));
    }

    #[test]
    fn without_external_storage_the_notebooks_stay_internal() {
        let internal = Path::new("/data/user/0/app/files");
        let layout = layout(Some(internal), None).expect("layout");
        assert_eq!(layout.notebooks, internal.join("NoteCraft"));
    }

    #[test]
    fn without_internal_storage_there_is_no_layout() {
        assert_eq!(layout(None, Some(Path::new("/storage/x"))), None);
    }
}

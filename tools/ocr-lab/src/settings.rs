//! What the lab remembers between runs: the test folder and the model folder. Kept in
//! `local/settings.json`, which is git-ignored, because the paths are particular to this machine.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub folder: Option<PathBuf>,
    pub models: Option<PathBuf>,
    /// The theme's id, such as `light-default` (see `notecraft_ui_egui::theme::Theme::id`).
    pub theme: Option<String>,
}

fn file() -> PathBuf {
    crate::lab_root().join("local").join("settings.json")
}

/// The saved settings, or none when there is no file or it cannot be read.
pub fn load() -> Settings {
    std::fs::read_to_string(file()).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

/// Saves the settings. A failure only means the next start asks again, so it is not reported.
pub fn save(settings: &Settings) {
    let path = file();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(path, format!("{text}\n"));
    }
}

/// The model folder is usable when both ONNX files are in it.
pub fn models_present(dir: &Path) -> bool {
    dir.join("text-detection.onnx").is_file() && dir.join("text-recognition.onnx").is_file()
}

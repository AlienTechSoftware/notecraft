//! The OCR model files: where they come from, what they should look like, and whether the copy on
//! this machine is present and intact. The lab shows the result as a light in its header.
//!
//! The files are the public `ocrs` models. Their weights carry the CC BY-SA 4.0 licence (see the
//! OCR skill). Nothing is downloaded unless a person presses the download button.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// One model file: where it is downloaded from, what it must look like, and what the lab's
/// Models section says about it. Kept together, so the description cannot drift from the files the
/// lab checks (issue #490).
pub struct ModelFile {
    pub name: &'static str,
    pub url: &'static str,
    pub size: u64,
    pub sha256: &'static str,
    /// What the model does in the reading, in a few words.
    pub role: &'static str,
    /// What it can read, and what it is given.
    pub capabilities: &'static str,
    /// The project page the model comes from.
    pub source: &'static str,
    pub licence: &'static str,
    pub licence_url: &'static str,
}

/// The model card of the ocrs weights (its licence and training data are stated there).
pub const MODEL_CARD: &str = "https://huggingface.co/robertknight/ocrs";

pub const DETECTION: &str = "text-detection.onnx";
pub const RECOGNITION: &str = "text-recognition.onnx";

pub const MODELS: [ModelFile; 2] = [
    ModelFile {
        name: DETECTION,
        url: "https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.onnx",
        size: 2_499_479,
        sha256: "a917b23dbd9524b465df7e922641b3ff2981623df4ded5a0234004ef2fee7cfe",
        role: "Detection: finds where the words are",
        capabilities: "Marks the pixels that are text, from which the words and lines are boxed. Any script, \
            as long as it looks like text. Reads the picture in greyscale, scaled to its 800 x 600 input.",
        source: "https://github.com/robertknight/ocrs-models",
        licence: "CC BY-SA 4.0",
        licence_url: "https://creativecommons.org/licenses/by-sa/4.0/",
    },
    ModelFile {
        name: RECOGNITION,
        url: "https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.onnx",
        size: 9_713_177,
        sha256: "86c145c2edb96c8caed5b1ebb8f44d706408922211451c309c625157dd6061c5",
        role: "Recognition: reads each line",
        capabilities: "Turns one line at a time into text, in greyscale at a fixed height. English only: the \
            Latin letters A to Z and a to z, digits, space and ASCII punctuation. No accents and no other \
            scripts (Arabic fails).",
        source: "https://github.com/robertknight/ocrs-models",
        licence: "CC BY-SA 4.0",
        licence_url: "https://creativecommons.org/licenses/by-sa/4.0/",
    },
];

/// What the Models section shows for one file: the file as the lab expects it, and its size in the
/// model folder when it is there.
pub struct Shown {
    pub model: &'static ModelFile,
    /// The size on disk, when the folder is set and the file is in it.
    pub on_disk: Option<u64>,
}

/// One row per model file, for the Models section.
pub fn shown(dir: Option<&Path>) -> Vec<Shown> {
    MODELS
        .iter()
        .map(|model| Shown { model, on_disk: dir.and_then(|dir| std::fs::metadata(dir.join(model.name)).ok()).map(|meta| meta.len()) })
        .collect()
}

/// A byte count as people read it: "2.5 MB".
pub fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

/// The state of a model folder.
#[derive(Debug, Clone, PartialEq)]
pub enum Health {
    /// No folder is chosen yet.
    NotSet,
    /// A file is absent.
    Missing(String),
    /// A file is there but its size or checksum is wrong. The reason says which.
    Corrupt(String),
    /// Both files are present and match.
    Ok,
}

impl Health {
    pub fn describe(&self) -> String {
        match self {
            Health::NotSet => "No model folder is set.".to_string(),
            Health::Missing(name) => format!("{name} is missing. Press Download models."),
            Health::Corrupt(reason) => format!("{reason} Press Download models to replace it."),
            Health::Ok => "Models present and intact.".to_string(),
        }
    }
}

fn sha256_of(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

/// Checks both files in `dir`: present, the expected size, and the expected checksum.
pub fn check(dir: Option<&Path>) -> Health {
    let Some(dir) = dir else { return Health::NotSet };
    for model in &MODELS {
        let path = dir.join(model.name);
        let Ok(meta) = std::fs::metadata(&path) else {
            return Health::Missing(model.name.to_string());
        };
        if meta.len() != model.size {
            return Health::Corrupt(format!("{} is {} bytes, expected {}.", model.name, meta.len(), model.size));
        }
        match sha256_of(&path) {
            Ok(hash) if hash == model.sha256 => {}
            Ok(_) => return Health::Corrupt(format!("{} does not match its checksum.", model.name)),
            Err(e) => return Health::Corrupt(e),
        }
    }
    Health::Ok
}

/// Downloads both files into `dir`, checks each one, and only then puts it in place, so a failed
/// download never leaves a half-written model behind.
pub fn download(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for model in &MODELS {
        // Two minutes for the whole request: a stalled connection must not hang the window for good.
        let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(120))).build().into();
        let mut response = agent.get(model.url).call().map_err(|e| format!("{}: {e}", model.url))?;
        let bytes = response.body_mut().read_to_vec().map_err(|e| format!("{}: {e}", model.url))?;
        let hash = format!("{:x}", Sha256::digest(&bytes));
        if bytes.len() as u64 != model.size || hash != model.sha256 {
            return Err(format!("{} came back with the wrong size or checksum; nothing was saved.", model.name));
        }
        let target: PathBuf = dir.join(model.name);
        let tmp = dir.join(format!("{}.part", model.name));
        std::fs::write(&tmp, &bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &target).map_err(|e| format!("{}: {e}", target.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_folder_is_not_set_and_an_empty_folder_is_missing() {
        assert_eq!(check(None), Health::NotSet);
        let dir = std::env::temp_dir().join(format!("ocr-lab-models-{}", crate::unique_suffix()));
        std::fs::create_dir_all(&dir).expect("dir");
        assert_eq!(check(Some(&dir)), Health::Missing("text-detection.onnx".to_string()));
        std::fs::remove_dir_all(&dir).expect("clean up");
    }

    #[test]
    fn a_file_of_the_wrong_size_is_corrupt() {
        let dir = std::env::temp_dir().join(format!("ocr-lab-models-size-{}", crate::unique_suffix()));
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("text-detection.onnx"), b"short").expect("file");
        assert!(matches!(check(Some(&dir)), Health::Corrupt(_)));
        std::fs::remove_dir_all(&dir).expect("clean up");
    }

    #[test]
    fn every_checked_file_has_its_description_and_the_reader_loads_those_files() {
        let names: Vec<&str> = MODELS.iter().map(|m| m.name).collect();
        assert_eq!(names, [DETECTION, RECOGNITION], "the files the lab checks are the files the reader loads");
        for model in &MODELS {
            assert!(model.url.ends_with(model.name), "{} is downloaded under its own name", model.name);
            assert_eq!(model.sha256.len(), 64, "{}: a SHA-256 is 64 hex digits", model.name);
            assert!(model.sha256.bytes().all(|b| b.is_ascii_hexdigit()));
            assert!(model.size > 0);
            for text in [model.role, model.capabilities, model.licence] {
                assert!(!text.trim().is_empty(), "{} has every field filled", model.name);
            }
            for link in [model.source, model.licence_url, MODEL_CARD] {
                assert!(link.starts_with("https://"), "{link} opens in the browser");
            }
        }
    }

    #[test]
    fn the_models_section_shows_each_file_with_its_size_on_disk_or_none() {
        let rows = shown(None);
        assert_eq!(rows.len(), MODELS.len());
        assert!(rows.iter().all(|r| r.on_disk.is_none()), "no folder: nothing on disk");
        let dir = std::env::temp_dir().join(format!("ocr-lab-models-shown-{}", crate::unique_suffix()));
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join(DETECTION), b"12345").expect("file");
        let rows = shown(Some(&dir));
        assert_eq!(rows[0].on_disk, Some(5));
        assert_eq!(rows[1].on_disk, None, "a missing file shows as missing");
        assert_eq!(megabytes(2_499_479), "2.5 MB");
        std::fs::remove_dir_all(&dir).expect("clean up");
    }
}

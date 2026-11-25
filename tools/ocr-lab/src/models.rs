//! The OCR model files: where they come from, what they should look like, and whether the copy on
//! this machine is present and intact. The lab shows the result as a light in its header.
//!
//! The files are the public `ocrs` models. Their weights carry the CC BY-SA 4.0 licence (see the
//! OCR skill). Nothing is downloaded unless a person presses the download button.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub struct ModelFile {
    pub name: &'static str,
    pub url: &'static str,
    pub size: u64,
    pub sha256: &'static str,
    /// What the model does in the OCR pipeline.
    pub role: &'static str,
    /// What the model can and cannot read, as measured or documented.
    pub capabilities: &'static str,
    /// Where the file's version is recorded, and what the header says about how it was made.
    pub version: &'static str,
    /// Measured cost, from the bake-off (see the OCR skill's references).
    pub perf: &'static str,
}

/// Versions of the code that runs the models, as pinned in Cargo.lock.
pub const ENGINE_VERSION: &str = "ocrs 0.14.0, rten 0.28.0";
/// Measured on the POC pictures, release build, one run each (bake-off 2026-11-22).
pub const PERF_SUMMARY: &str = "Load of both models about 6 ms. One picture 109 to 154 ms, including decoding, on a Windows development machine";

/// Where the models and the engine come from. Shown in the lab's details pane and opened by its links.
pub const ENGINE_NAME: &str = "ocrs 0.14 (robertknight/ocrs)";
pub const ENGINE_SOURCE: &str = "https://github.com/robertknight/ocrs";
/// The neural-network runtime that runs the ONNX models. Pure Rust, from the same author as `ocrs`.
pub const INFERENCE_RUNTIME: &str = "rten 0.28 (pure Rust), runs ONNX model files";
/// The model family: the official ocrs detection and recognition models, both in ONNX format.
pub const MODEL_FAMILY: &str = "ocrs official models (ONNX format): text detection and text recognition";
pub const INFERENCE_SOURCE: &str = "https://crates.io/crates/rten";
pub const MODEL_REPO: &str = "https://github.com/robertknight/ocrs-models";
pub const MODEL_CARD: &str = "https://huggingface.co/robertknight/ocrs";
/// The weights' licence, from the model card. Not shipped with the product until decided.
pub const LICENCE: &str = "CC BY-SA 4.0 (weights); the ocrs crate is MIT or Apache-2.0";

pub const MODELS: [ModelFile; 2] = [
    ModelFile {
        name: "text-detection.onnx",
        url: "https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.onnx",
        size: 2_499_479,
        sha256: "a917b23dbd9524b465df7e922641b3ff2981623df4ded5a0234004ef2fee7cfe",
        role: "Finds text lines and their boxes in the picture",
        capabilities: "Printed text lines; ocrs gives no per-line confidence score",
        version: "No version field in the name; identified by size and SHA-256. Header says rten-convert",
        perf: "Part of the 109 to 154 ms per picture above",
    },
    ModelFile {
        name: "text-recognition.onnx",
        url: "https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.onnx",
        size: 9_713_177,
        sha256: "86c145c2edb96c8caed5b1ebb8f44d706408922211451c309c625157dd6061c5",
        role: "Reads the characters inside each text line",
        capabilities: "English alphabet only; Arabic measured to fail (bake-off 2026-11-22)",
        version: "No version field in the name; identified by size and SHA-256. Header says rten-convert",
        perf: "Part of the 109 to 154 ms per picture above",
    },
];

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
}

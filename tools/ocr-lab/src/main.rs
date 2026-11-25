//! NoteCraft OCR lab: a manual test and label tool for OCR. It runs the product's own OCR code
//! (`notecraft-ocr`) on a folder of pictures, shows what it reads over the picture, and lets a
//! person draw and correct the true boxes and text. It is for testing only and is never shipped.
//!
//! Usage: `cargo run -p notecraft-ocr-lab -- [--folder <dir>] [--models <dir>]`
//! `--folder` defaults to the OCR fixtures in the repository. `--models` (or the environment
//! variable NOTECRAFT_OCR_MODELS) is the folder holding `text-detection.onnx` and
//! `text-recognition.onnx`.

mod app;
mod case;
mod metrics;
mod models;
mod pen;
mod settings;
mod telemetry;

use eframe::egui;
use std::path::{Path, PathBuf};

/// A name for a scratch folder that no other test run shares: the process ID plus the time now.
#[cfg(test)]
pub fn unique_suffix() -> String {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{}-{nanos}", std::process::id())
}

/// This tool's own folder (its `local` output lives under it).
pub fn lab_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn argument(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn default_folder() -> PathBuf {
    lab_root().join("..").join("..").join("crates").join("ocr").join("tests").join("fixtures").join("mvp-smoke")
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    let saved = settings::load();
    // Flags win, then what was saved last time, then the environment, then the repository fixtures.
    let folder = argument(&args, "--folder").map(PathBuf::from).or_else(|| saved.folder.clone()).unwrap_or_else(default_folder);
    let models = argument(&args, "--models")
        .map(PathBuf::from)
        .or_else(|| saved.models.clone())
        .or_else(|| std::env::var("NOTECRAFT_OCR_MODELS").ok().map(PathBuf::from));
    let log = telemetry::Log::open(&lab_root().join("local").join("ocr-lab.log"));
    if args.iter().any(|a| a == "--check") {
        std::process::exit(check(&folder, models.as_deref(), log));
    }
    settings::save(&settings::Settings {
        folder: Some(folder.clone()),
        models: models.clone().filter(|m| settings::models_present(m)).or(saved.models.clone()),
        theme: saved.theme.clone(),
        follow_system: saved.follow_system,
    });
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1400.0, 900.0]).with_title("NoteCraft OCR lab").with_decorations(false),
        ..Default::default()
    };
    eframe::run_native(
        "NoteCraft OCR lab",
        options,
        Box::new(move |_cc| Ok(Box::new(Shell(app::Lab::new(folder, models, saved.theme.clone(), saved.follow_system, log))))),
    )
}

/// Headless run for a test folder: reads every picture with the product's engine, scores it
/// against its label file and prints one line per picture. Exits 1 when a picture cannot be read.
fn check(folder: &Path, models: Option<&Path>, mut log: telemetry::Log) -> i32 {
    let Some(models) = models else {
        eprintln!("--check needs --models <folder> (or NOTECRAFT_OCR_MODELS)");
        return 2;
    };
    let mut timings = Vec::new();
    let reader = match telemetry::timed("model load", &mut timings, || {
        notecraft_ocr::OcrsReader::from_model_files(&models.join("text-detection.onnx"), &models.join("text-recognition.onnx"))
    }) {
        Ok(reader) => reader,
        Err(e) => {
            eprintln!("models did not load: {e:?}");
            return 2;
        }
    };
    let mut failures = 0;
    let pictures = case::discover(folder);
    if pictures.is_empty() {
        eprintln!("no pictures found in {}", folder.display());
        return 1;
    }
    for picture in &pictures {
        let name = picture.strip_prefix(folder).unwrap_or(picture).to_string_lossy().replace('\\', "/");
        let labels = match case::load_labels(picture) {
            Ok(file) => file.labels,
            Err(e) => {
                failures += 1;
                println!("{name}: FAILED the label file: {e}");
                continue;
            }
        };
        let mut run = Vec::new();
        let read = telemetry::timed("read picture", &mut run, || reader.read_image(picture));
        match read {
            Ok(lines) => {
                let score = case::score(&labels, &lines);
                let ms = run.first().map_or(0.0, |t| t.ms);
                println!(
                    "{name}: {} lines, labels {} found {} text {}, unmatched lines {}, {ms:.0} ms",
                    lines.len(),
                    score.labels,
                    score.found,
                    score.text_matched,
                    score.unmatched_engine_lines
                );
                log.event(&format!("check {name}: {} lines in {ms:.0} ms", lines.len()));
            }
            Err(e) => {
                failures += 1;
                println!("{name}: FAILED {e:?}");
                log.event(&format!("check {name} failed: {e:?}"));
            }
        }
    }
    println!("{} pictures, {failures} failed", pictures.len());
    i32::from(failures > 0)
}

struct Shell(app::Lab);

impl eframe::App for Shell {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.ui(ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flag_value_is_the_word_after_the_flag() {
        let args: Vec<String> = ["lab", "--folder", "cases", "--models"].iter().map(|s| s.to_string()).collect();
        assert_eq!(argument(&args, "--folder").as_deref(), Some("cases"));
        assert_eq!(argument(&args, "--models"), None, "a flag with no value gives none");
        assert_eq!(argument(&args, "--missing"), None);
    }

    #[test]
    fn the_headless_check_says_it_needs_models_and_fails_without_them() {
        let dir = std::env::temp_dir().join(format!("ocr-lab-check-{}", crate::unique_suffix()));
        std::fs::create_dir_all(&dir).expect("dir");
        let log = telemetry::Log::open(&dir.join("log.txt"));
        assert_eq!(check(&dir, None, log), 2, "no models given");
        let log = telemetry::Log::open(&dir.join("log.txt"));
        assert_eq!(check(&dir, Some(&dir), log), 2, "a folder with no model files does not load");
        std::fs::remove_dir_all(&dir).expect("clean up");
    }
}

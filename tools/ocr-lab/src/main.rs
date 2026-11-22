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
mod telemetry;

use eframe::egui;
use std::path::{Path, PathBuf};

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
    let folder = argument(&args, "--folder").map(PathBuf::from).unwrap_or_else(default_folder);
    let models = argument(&args, "--models").or_else(|| std::env::var("NOTECRAFT_OCR_MODELS").ok()).map(PathBuf::from);
    let log = telemetry::Log::open(&lab_root().join("local").join("ocr-lab.log"));
    if args.iter().any(|a| a == "--check") {
        std::process::exit(check(&folder, models.as_deref(), log));
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1400.0, 900.0]).with_title("NoteCraft OCR lab"),
        ..Default::default()
    };
    eframe::run_native("NoteCraft OCR lab", options, Box::new(move |_cc| Ok(Box::new(Shell(app::Lab::new(folder, models, log))))))
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
    for picture in &pictures {
        let name = picture.strip_prefix(folder).unwrap_or(picture).to_string_lossy().replace('\\', "/");
        let labels = case::load_labels(picture).map(|f| f.labels).unwrap_or_default();
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

//! Printed-text OCR for pictures in a page's `assets/` folder: the text of each line and its
//! box (four corners in picture pixels).
//!
//! Stage 1 MVP. The engine is `ocrs` (pure Rust, MIT or Apache-2.0), chosen for the bake-off
//! by the owner's GO. Its model files are not part of this crate; the caller passes their paths.

use std::path::Path;

use image::GenericImageView;
use ocrs::TextItem;

/// A corner of a line's box, in picture pixels, origin at the top left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// One line of text read from a picture.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub text: String,
    /// The four corners of the line's box: top left, top right, bottom right, bottom left.
    pub quad: [Point; 4],
    /// Confidence in `0.0..=1.0`, or `None` when the engine does not report one. `ocrs` 0.14
    /// does not expose a per-line confidence, so its lines carry `None`.
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OcrError {
    /// The picture could not be decoded.
    Decode(String),
    /// A model file could not be loaded.
    Model(String),
    /// The OCR engine failed.
    Engine(String),
    /// A confidence outside `0.0..=1.0`.
    InvalidConfidence(f32),
}

impl Line {
    /// Builds a line, refusing a confidence outside `0.0..=1.0` (criterion A2).
    pub fn new(text: impl Into<String>, quad: [Point; 4], confidence: Option<f32>) -> Result<Line, OcrError> {
        if let Some(c) = confidence
            && !(0.0..=1.0).contains(&c)
        {
            return Err(OcrError::InvalidConfidence(c));
        }
        Ok(Line { text: text.into(), quad, confidence })
    }
}

/// The two `ocrs` models, loaded once and reused for every picture. The models are the official
/// ONNX files (`text-detection.onnx`, `text-recognition.onnx`), loaded by `rten` in pure Rust.
pub struct Engine {
    ocr: ocrs::OcrEngine,
}

impl Engine {
    /// Loads the detection and recognition models from their `.rten` files.
    pub fn from_model_files(detection: &Path, recognition: &Path) -> Result<Engine, OcrError> {
        let load = |path: &Path| rten::Model::load_file(path).map_err(|e| OcrError::Model(format!("{}: {e}", path.display())));
        let ocr = ocrs::OcrEngine::new(ocrs::OcrEngineParams {
            detection_model: Some(load(detection)?),
            recognition_model: Some(load(recognition)?),
            ..Default::default()
        })
        .map_err(|e| OcrError::Engine(e.to_string()))?;
        Ok(Engine { ocr })
    }

    /// Reads the printed-text lines of one picture, in reading order (criteria A1 and A3).
    pub fn read_image(&self, path: &Path) -> Result<Vec<Line>, OcrError> {
        let decoded = image::open(path).map_err(|e| OcrError::Decode(format!("{}: {e}", path.display())))?;
        let (width, height) = decoded.dimensions();
        let rgb = decoded.into_rgb8();
        let source = ocrs::ImageSource::from_bytes(rgb.as_raw(), (width, height)).map_err(|e| OcrError::Decode(e.to_string()))?;
        let engine = |e: ocrs::OcrError| OcrError::Engine(e.to_string());
        let input = self.ocr.prepare_input(source).map_err(engine)?;
        let words = self.ocr.detect_words(&input).map_err(engine)?;
        let lines = self.ocr.find_text_lines(&input, &words);
        let texts = self.ocr.recognize_text(&input, &lines).map_err(engine)?;

        let mut out = Vec::new();
        for text_line in texts.into_iter().flatten() {
            let text = text_line.to_string();
            if text.trim().is_empty() {
                continue;
            }
            let (mut left, mut top, mut right, mut bottom) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
            for ch in text_line.chars() {
                left = left.min(ch.rect.left());
                top = top.min(ch.rect.top());
                right = right.max(ch.rect.right());
                bottom = bottom.max(ch.rect.bottom());
            }
            // The engine can return a box a few pixels past the picture edge; a box lies inside the picture (A3).
            let (w, h) = (width as i32, height as i32);
            let p = |x: i32, y: i32| Point { x: x.clamp(0, w) as f32, y: y.clamp(0, h) as f32 };
            let quad = [p(left, top), p(right, top), p(right, bottom), p(left, bottom)];
            out.push(Line::new(text.trim().to_string(), quad, None)?);
        }
        Ok(out)
    }
}

/// What the companion job did for one picture (criteria A4 and A5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refresh {
    /// The picture's hash matched the stored one; nothing was read.
    Unchanged,
    /// The picture was read again and its companion text replaced.
    Rewritten,
}

/// Re-reads a picture only when its file hash changed since the last read (criteria A4 and A5).
pub fn refresh_companion(_picture: &Path) -> Result<Refresh, OcrError> {
    panic!("A4/A5: hash check and companion replacement are not implemented yet")
}

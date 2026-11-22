//! Printed-text OCR for pictures in a page's `assets/` folder: the text of each line, its box
//! (a quadrilateral in picture pixels) and a confidence from 0 to 1.
//!
//! Stage 1 MVP. The engine is not chosen yet (owner decision, plan section 6), so the entry
//! points below are stubs that fail with the criterion they serve.

use std::path::Path;

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
    /// The four corners of the line's box, in reading order.
    pub quad: [Point; 4],
    /// Confidence in `0.0..=1.0`.
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OcrError {
    /// The entry point exists but its behaviour is not implemented yet.
    NotImplemented(&'static str),
    /// The picture could not be decoded.
    Decode(String),
    /// The OCR engine failed.
    Engine(String),
    /// A confidence outside `0.0..=1.0`.
    InvalidConfidence(f32),
}

impl Line {
    /// Builds a line, refusing a confidence outside `0.0..=1.0` (criterion A2).
    pub fn new(text: impl Into<String>, quad: [Point; 4], confidence: f32) -> Result<Line, OcrError> {
        let _ = (text.into(), quad, confidence);
        panic!("A2: validate that the confidence is in 0.0..=1.0")
    }
}

/// Reads the printed-text lines of one picture, in reading order (criteria A1 and A3).
pub fn read_image(_path: &Path) -> Result<Vec<Line>, OcrError> {
    panic!("A1/A3: the OCR engine is not chosen yet (plan section 6)")
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
    panic!("A4/A5: hash check and companion replacement")
}

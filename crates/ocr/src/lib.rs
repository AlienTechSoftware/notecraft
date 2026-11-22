//! Printed-text OCR for pictures in a page's `assets/` folder: the text of each line and its
//! box (four corners in picture pixels).
//!
//! Stage 1 MVP. The engine is `ocrs` (pure Rust, MIT or Apache-2.0), chosen for the bake-off
//! by the owner's GO. Its model files are not part of this crate; the caller passes their paths.

use std::path::{Path, PathBuf};

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

/// Anything that reads the printed-text lines of a picture. The companion job depends on this
/// trait, not on a particular engine, so a new engine (a platform fast path, PP-OCRv5 through ONNX,
/// a test double) plugs in without changing the job.
pub trait PictureReader {
    /// Reads one picture, lines in reading order.
    fn read_picture(&self, path: &Path) -> Result<Vec<Line>, OcrError>;
}

impl PictureReader for OcrsReader {
    fn read_picture(&self, path: &Path) -> Result<Vec<Line>, OcrError> {
        self.read_image(path)
    }
}

/// The two `ocrs` models, loaded once and reused for every picture. The models are the official
/// ONNX files (`text-detection.onnx`, `text-recognition.onnx`), loaded by `rten` in pure Rust.
pub struct OcrsReader {
    ocr: ocrs::OcrEngine,
}

impl OcrsReader {
    /// Loads the detection and recognition models from their `.rten` files.
    pub fn from_model_files(detection: &Path, recognition: &Path) -> Result<OcrsReader, OcrError> {
        let load = |path: &Path| rten::Model::load_file(path).map_err(|e| OcrError::Model(format!("{}: {e}", path.display())));
        let ocr = ocrs::OcrEngine::new(ocrs::OcrEngineParams {
            detection_model: Some(load(detection)?),
            recognition_model: Some(load(recognition)?),
            ..Default::default()
        })
        .map_err(|e| OcrError::Engine(e.to_string()))?;
        Ok(OcrsReader { ocr })
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

        let mut pieces = Vec::new();
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
            pieces.push(Piece { text: text.trim().to_string(), left, top, right, bottom });
        }
        // The engine splits a row of columns (an item and its price) into separate lines. Join the
        // pieces that share a row, left to right, so each printed row is one line (A1).
        let (w, h) = (width as i32, height as i32);
        let p = |x: i32, y: i32| Point { x: x.clamp(0, w) as f32, y: y.clamp(0, h) as f32 };
        let mut out = Vec::new();
        for row in group_rows(pieces) {
            let quad = [p(row.left, row.top), p(row.right, row.top), p(row.right, row.bottom), p(row.left, row.bottom)];
            out.push(Line::new(row.text, quad, None)?);
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
///
/// The text goes to `<picture>.txt` beside the picture, one line per row. The hash of the bytes
/// it was read from goes to `<picture>.ocr-hash`. Both are written through a temporary file, so a
/// crash leaves the old pair, never a half-written one.
pub fn refresh_companion(reader: &dyn PictureReader, picture: &Path) -> Result<Refresh, OcrError> {
    let bytes = std::fs::read(picture).map_err(|e| OcrError::Decode(format!("{}: {e}", picture.display())))?;
    let hash = format!("{:016x}", fnv1a(&bytes));
    let text_path = companion_path(picture, "txt");
    let hash_path = companion_path(picture, "ocr-hash");
    if text_path.exists() && std::fs::read_to_string(&hash_path).is_ok_and(|stored| stored.trim() == hash) {
        return Ok(Refresh::Unchanged);
    }
    let lines = reader.read_picture(picture)?;
    let text: String = lines.iter().map(|l| format!("{}\n", l.text)).collect();
    write_replacing(&text_path, text.as_bytes())?;
    write_replacing(&companion_path(picture, "lines"), &lines_file(&lines))?;
    write_replacing(&hash_path, hash.as_bytes())?;
    Ok(Refresh::Rewritten)
}

/// The boxes file: one line per text line, `x0,y0,x1,y1,x2,y2,x3,y3<TAB>text`, in picture pixels.
/// Tabs and line breaks inside a text are replaced by spaces, so each line is one record.
fn lines_file(lines: &[Line]) -> Vec<u8> {
    let mut out = String::new();
    for line in lines {
        let coords: Vec<String> = line.quad.iter().flat_map(|p| [p.x.to_string(), p.y.to_string()]).collect();
        let text: String = line.text.chars().map(|c| if c == '\t' || c == '\n' || c == '\r' { ' ' } else { c }).collect();
        out.push_str(&coords.join(","));
        out.push('\t');
        out.push_str(&text);
        out.push('\n');
    }
    out.into_bytes()
}

fn companion_path(picture: &Path, extension: &str) -> PathBuf {
    let mut name = picture.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(extension);
    picture.with_file_name(name)
}

fn write_replacing(path: &Path, bytes: &[u8]) -> Result<(), OcrError> {
    let temp = companion_path(path, "tmp");
    std::fs::write(&temp, bytes).map_err(|e| OcrError::Decode(format!("{}: {e}", temp.display())))?;
    std::fs::rename(&temp, path).map_err(|e| OcrError::Decode(format!("{}: {e}", path.display())))
}

/// FNV-1a, 64 bits: a hash that is the same on every run and every platform, which the standard
/// library's hasher is not.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, b| (hash ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3))
}

/// One piece of text as the engine reports it, before rows are joined.
struct Piece {
    text: String,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

/// Joins pieces that overlap vertically by at least half the shorter one into one row, in reading
/// order: rows top to bottom, pieces left to right inside a row.
fn group_rows(mut pieces: Vec<Piece>) -> Vec<Piece> {
    pieces.sort_by_key(|p| (p.top, p.left));
    let mut rows: Vec<Vec<Piece>> = Vec::new();
    for piece in pieces {
        let joins = rows.iter().position(|row| row.iter().any(|other| overlaps_vertically(other, &piece)));
        match joins {
            Some(i) => rows[i].push(piece),
            None => rows.push(vec![piece]),
        }
    }
    rows.into_iter()
        .map(|mut row| {
            row.sort_by_key(|p| p.left);
            Piece {
                text: row.iter().map(|p| p.text.as_str()).collect::<Vec<_>>().join(" "),
                left: row.iter().map(|p| p.left).min().unwrap_or(0),
                top: row.iter().map(|p| p.top).min().unwrap_or(0),
                right: row.iter().map(|p| p.right).max().unwrap_or(0),
                bottom: row.iter().map(|p| p.bottom).max().unwrap_or(0),
            }
        })
        .collect()
}

fn overlaps_vertically(a: &Piece, b: &Piece) -> bool {
    let overlap = a.bottom.min(b.bottom) - a.top.max(b.top);
    let shorter = (a.bottom - a.top).min(b.bottom - b.top).max(1);
    overlap * 2 >= shorter
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// A reader that counts its calls and returns one fixed line. It needs no model files.
    struct CountingReader {
        calls: Cell<u32>,
    }

    impl PictureReader for CountingReader {
        fn read_picture(&self, _path: &Path) -> Result<Vec<Line>, OcrError> {
            self.calls.set(self.calls.get() + 1);
            Ok(vec![Line::new("TOTAL 13.75", [Point { x: 0.0, y: 0.0 }; 4], None)?])
        }
    }

    #[test]
    fn the_companion_job_works_with_any_reader_and_reads_only_on_change() {
        let dir = std::env::temp_dir().join(format!("nc-ocr-unit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch folder");
        let picture = dir.join("picture.png");
        std::fs::write(&picture, b"first bytes").expect("picture");
        let reader = CountingReader { calls: Cell::new(0) };

        assert_eq!(refresh_companion(&reader, &picture).expect("first"), Refresh::Rewritten);
        assert_eq!(refresh_companion(&reader, &picture).expect("second"), Refresh::Unchanged);
        assert_eq!(reader.calls.get(), 1, "an unchanged picture is not read again");

        std::fs::write(&picture, b"second bytes").expect("change");
        assert_eq!(refresh_companion(&reader, &picture).expect("third"), Refresh::Rewritten);
        assert_eq!(reader.calls.get(), 2);
        assert_eq!(std::fs::read_to_string(dir.join("picture.png.txt")).expect("companion"), "TOTAL 13.75\n");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_boxes_file_has_one_record_per_line_and_no_stray_breaks() {
        let quad = [Point { x: 0.0, y: 0.0 }, Point { x: 10.0, y: 0.0 }, Point { x: 10.0, y: 4.0 }, Point { x: 0.0, y: 4.0 }];
        let lines = vec![
            Line::new(
                "a	b
c", quad, None,
            )
            .expect("line"),
        ];
        assert_eq!(
            String::from_utf8(lines_file(&lines)).expect("utf-8"),
            "0,0,10,0,10,4,0,4	a b c
"
        );
    }
}

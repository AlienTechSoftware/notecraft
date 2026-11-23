//! OCR on the two CC0 samples the demo shows: a printed Arabic receipt with a handwritten total,
//! and a handwritten Arabic page. The pictures are in `assets/demo`, so this runs without the
//! private dataset. It needs the models: set `NOTECRAFT_OCR_MODELS` and run with `--ignored`.
//!
//! What is checked: each picture reads without error, the boxes lie inside it, and the receipt
//! gives lines. The text is not compared with a truth yet: the recogniser is English, so the
//! Arabic lines are measured, not asserted (see the OCR skill, bake-off results).

use std::path::PathBuf;

use notecraft_ocr::OcrsReader;

fn sample(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("assets").join("demo").join(name)
}

fn engine() -> OcrsReader {
    let models = std::env::var_os("NOTECRAFT_OCR_MODELS").map(PathBuf::from).expect("set NOTECRAFT_OCR_MODELS");
    OcrsReader::from_model_files(&models.join("text-detection.onnx"), &models.join("text-recognition.onnx")).expect("models load")
}

fn check_boxes_inside(path: &PathBuf, engine: &OcrsReader) -> usize {
    let (width, height) = image::image_dimensions(path).expect("picture size");
    let lines = engine.read_image(path).expect("the picture reads");
    for line in &lines {
        for p in line.quad {
            assert!(p.x >= 0.0 && p.x <= width as f32 && p.y >= 0.0 && p.y <= height as f32, "{}: a box lies outside", path.display());
        }
    }
    lines.len()
}

/// The receipt is a printed receipt: the engine finds lines on it, with boxes inside the picture.
#[test]
#[ignore = "needs NOTECRAFT_OCR_MODELS; run with --ignored"]
fn the_arabic_receipt_photo_reads_with_boxes_inside() {
    let engine = engine();
    let lines = check_boxes_inside(&sample("ocr-receipt-ar-photo.jpg"), &engine);
    assert!(lines >= 3, "the receipt gives several lines, got {lines}");
}

/// The handwritten page reads without error and its boxes lie inside the picture. Handwriting is
/// a later stage, so no line count is asserted here.
#[test]
#[ignore = "needs NOTECRAFT_OCR_MODELS; run with --ignored"]
fn the_handwritten_letter_reads_without_error_and_boxes_stay_inside() {
    let engine = engine();
    check_boxes_inside(&sample("ocr-letter-ar.jpg"), &engine);
}

/// The two samples are real files of the expected kind, and their sizes are recorded, so a
/// changed or truncated copy is caught even with no models.
#[test]
fn the_samples_are_real_jpeg_files_of_the_expected_size() {
    for (name, min_bytes) in [("ocr-receipt-ar-photo.jpg", 20_000u64), ("ocr-letter-ar.jpg", 50_000u64)] {
        let path = sample(name);
        let bytes = std::fs::read(&path).expect("the sample is there");
        assert!(bytes.len() as u64 >= min_bytes, "{name} is {} bytes", bytes.len());
        assert_eq!(&bytes[..3], &[0xFF, 0xD8, 0xFF], "{name} starts like a JPEG");
        image::image_dimensions(&path).expect("it has a size");
    }
}

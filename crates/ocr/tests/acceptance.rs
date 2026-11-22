//! Acceptance tests for Stage 1 (plan section 5). Written before the implementation, so they
//! fail until it exists. They read the POC pictures from `NOTECRAFT_OCR_FIXTURES`, which points
//! at the POC notebook's `Images` folder. The private notebook is not in the public repository.

use std::path::PathBuf;

use notecraft_ocr::{Line, OcrError, Point, Refresh, read_image, refresh_companion};

fn fixture(relative: &str) -> PathBuf {
    let root = std::env::var_os("NOTECRAFT_OCR_FIXTURES").map(PathBuf::from).expect("set NOTECRAFT_OCR_FIXTURES to the POC notebook's Images folder");
    root.join(relative)
}

fn corners() -> [Point; 4] {
    [Point { x: 0.0, y: 0.0 }, Point { x: 10.0, y: 0.0 }, Point { x: 10.0, y: 4.0 }, Point { x: 0.0, y: 4.0 }]
}

/// A2: each line has a box and a confidence in 0 to 1.
#[test]
fn a2_a_line_has_a_box_and_a_confidence_from_0_to_1() {
    let line = Line::new("total 12.40", corners(), 0.9).expect("a valid line");
    assert!((0.0..=1.0).contains(&line.confidence));
    assert_eq!(line.quad.len(), 4);
    assert_eq!(Line::new("total", corners(), 1.5), Err(OcrError::InvalidConfidence(1.5)));
}

/// A1: a printed picture yields its text lines, in reading order, matching the reference text.
#[test]
fn a1_the_receipt_reads_back_its_reference_text_in_reading_order() {
    let lines = read_image(&fixture("receipt-photo/assets/receipt.png")).expect("the receipt reads");
    let got: Vec<String> = lines.iter().map(|l| l.text.split_whitespace().collect::<Vec<_>>().join(" ")).collect();
    let reference = std::fs::read_to_string(fixture("receipt-photo/assets/receipt.png.txt")).expect("reference text");
    let want: Vec<String> = reference.lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|l| !l.is_empty()).collect();
    assert_eq!(got, want);
}

/// A3: every line's box lies inside the picture, for every POC picture.
#[test]
fn a3_every_box_lies_inside_its_picture() {
    let pictures = [
        "receipt-photo/assets/receipt.png",
        "chart-bars/assets/chart.png",
        "scanned-page/assets/scan.png",
        "whiteboard-sketch/assets/sketch.png",
        "arabic-alt-text/assets/receipt-ar.png",
    ];
    for relative in pictures {
        let path = fixture(relative);
        let (width, height) = image::image_dimensions(&path).expect("picture size");
        for line in read_image(&path).expect("picture reads") {
            for p in line.quad {
                assert!(p.x >= 0.0 && p.x <= width as f32, "{relative}: x {} outside {width}", p.x);
                assert!(p.y >= 0.0 && p.y <= height as f32, "{relative}: y {} outside {height}", p.y);
            }
        }
    }
}

/// A4: re-running OCR on an unchanged picture does nothing.
#[test]
fn a4_an_unchanged_picture_is_not_read_again() {
    let path = fixture("receipt-photo/assets/receipt.png");
    refresh_companion(&path).expect("first read");
    assert_eq!(refresh_companion(&path).expect("second read"), Refresh::Unchanged);
}

/// A5: changing a picture re-runs OCR and replaces its companion text.
#[test]
fn a5_a_changed_picture_is_read_again_and_its_companion_is_replaced() {
    let dir = std::env::temp_dir().join(format!("nc-ocr-a5-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch folder");
    let picture = dir.join("receipt.png");
    std::fs::copy(fixture("receipt-photo/assets/receipt.png"), &picture).expect("copy picture");
    refresh_companion(&picture).expect("first read");
    let mut bytes = std::fs::read(&picture).expect("read picture");
    bytes.extend_from_slice(b"changed");
    std::fs::write(&picture, bytes).expect("change picture");
    assert_eq!(refresh_companion(&picture).expect("second read"), Refresh::Rewritten);
    std::fs::remove_dir_all(&dir).ok();
}

//! Tests for the MVP 1 dataset: the manifest, the truth files and the smoke run.
//!
//! The dataset is private and is not in the repository. `NOTECRAFT_OCR_DATASET` points at its
//! folder (`smoke`, `dev`, `eval`, `truth` and `manifest.csv`). Without it, the tests that need
//! the dataset are ignored; run them with `--ignored` once it is set. The parser test always runs.
//!
//! The rules come from the dataset README: every picture has a manifest row with its source,
//! licence and credit before it is used; ids are unique; ground truth is written by a person.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use notecraft_ocr::{Line, OcrsReader};

/// Splits one CSV record into its fields. A quoted field may hold commas and doubled quotes.
fn split_record(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => fields.push(std::mem::take(&mut field)),
            (c, _) => field.push(c),
        }
    }
    fields.push(field);
    fields
}

/// One manifest row, by column name.
#[derive(Debug, Clone)]
struct Row {
    id: String,
    file: String,
    split: String,
    category: String,
    source: String,
    licence: String,
    credit: String,
    truth_file: String,
}

fn parse_manifest(text: &str) -> Vec<Row> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let header = split_record(lines.next().expect("the manifest has a header"));
    let column = |name: &str| header.iter().position(|h| h == name).unwrap_or_else(|| panic!("the manifest has a {name} column"));
    let (id, file, split, category) = (column("id"), column("file"), column("split"), column("category"));
    let (source, licence, credit, truth) = (column("source"), column("licence"), column("credit"), column("truth_file"));
    lines
        .map(|line| {
            let f = split_record(line);
            let get = |i: usize| f.get(i).cloned().unwrap_or_default();
            Row {
                id: get(id),
                file: get(file),
                split: get(split),
                category: get(category),
                source: get(source),
                licence: get(licence),
                credit: get(credit),
                truth_file: get(truth),
            }
        })
        .collect()
}

fn dataset() -> Option<PathBuf> {
    std::env::var_os("NOTECRAFT_OCR_DATASET").map(PathBuf::from)
}

fn load_manifest(root: &Path) -> Vec<Row> {
    parse_manifest(&std::fs::read_to_string(root.join("manifest.csv")).expect("manifest.csv is in the dataset"))
}

fn pictures_in(folder: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> =
        std::fs::read_dir(folder).map(|entries| entries.filter_map(|e| e.ok().map(|e| e.path())).collect()).unwrap_or_default();
    out.retain(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("png" | "jpg" | "jpeg")));
    out.sort();
    out
}

/// Parsing the manifest is checked on its own, so the rule is tested even with no dataset.
#[test]
fn a_manifest_record_keeps_commas_inside_quotes() {
    let text = "id,file,split,category,language,source,licence,credit,truth_file,notes\n\
                p-001,smoke/receipt.png,smoke,printed-ocr,en,\"own, drawn\",CC0,\"A. Person, a credit\",p-001.txt,\"said \"\"ok\"\"\"\n";
    let rows = parse_manifest(text);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].source, "own, drawn");
    assert_eq!(rows[0].credit, "A. Person, a credit");
    assert_eq!(rows[0].truth_file, "p-001.txt");
}

/// Every picture under smoke, dev and eval has a manifest row in its own split, with its source,
/// licence and credit filled in. Ids are unique.
#[test]
#[ignore = "needs NOTECRAFT_OCR_DATASET; run with --ignored"]
fn every_picture_has_a_complete_manifest_row_in_its_split() {
    let root = dataset().expect("set NOTECRAFT_OCR_DATASET");
    let rows = load_manifest(&root);
    let mut ids = HashSet::new();
    for row in &rows {
        assert!(ids.insert(row.id.clone()), "the id {} is used twice", row.id);
        assert!(
            !row.source.trim().is_empty() && !row.licence.trim().is_empty() && !row.credit.trim().is_empty(),
            "{} needs a source, a licence and a credit",
            row.id
        );
    }
    for split in ["smoke", "dev", "eval"] {
        for picture in pictures_in(&root.join(split)) {
            let name = picture.strip_prefix(&root).expect("inside the dataset").to_string_lossy().replace('\\', "/");
            let row = rows.iter().find(|r| r.file == name).unwrap_or_else(|| panic!("{name} has no manifest row (rule 2)"));
            assert_eq!(row.split, split, "{name} is listed under the wrong split");
        }
    }
}

/// Every printed-OCR row that has a truth file points at a file that exists and is not empty.
#[test]
#[ignore = "needs NOTECRAFT_OCR_DATASET; run with --ignored"]
fn every_printed_row_has_its_truth_file() {
    let root = dataset().expect("set NOTECRAFT_OCR_DATASET");
    for row in load_manifest(&root).iter().filter(|r| r.category == "printed-ocr") {
        let truth = root.join("truth").join(&row.truth_file);
        let text = std::fs::read_to_string(&truth).unwrap_or_default();
        assert!(!text.trim().is_empty(), "{} has no truth in {}", row.id, truth.display());
    }
}

/// The smoke set for printed OCR: each picture reads without error, and every box lies inside
/// its picture (criterion A3 on new pictures).
#[test]
#[ignore = "needs NOTECRAFT_OCR_DATASET and NOTECRAFT_OCR_MODELS; run with --ignored"]
fn the_printed_smoke_pictures_read_and_their_boxes_are_inside() {
    let root = dataset().expect("set NOTECRAFT_OCR_DATASET");
    let models = std::env::var_os("NOTECRAFT_OCR_MODELS").map(PathBuf::from).expect("set NOTECRAFT_OCR_MODELS");
    let engine = OcrsReader::from_model_files(&models.join("text-detection.onnx"), &models.join("text-recognition.onnx")).expect("models load");
    let rows = load_manifest(&root);
    let smoke: Vec<&Row> = rows.iter().filter(|r| r.split == "smoke" && r.category == "printed-ocr").collect();
    assert!(!smoke.is_empty(), "the smoke set has printed pictures");
    for row in smoke {
        let path = root.join(&row.file);
        let (width, height) = image::image_dimensions(&path).expect("picture size");
        let lines: Vec<Line> = engine.read_image(&path).expect("the picture reads");
        for line in lines {
            for p in line.quad {
                assert!(p.x >= 0.0 && p.x <= width as f32 && p.y >= 0.0 && p.y <= height as f32, "{}: a box lies outside the picture", row.id);
            }
        }
    }
}

//! Test cases: the pictures in a folder, their label files, and how the engine's lines score
//! against the labels. Nothing here is shipped; it only serves the lab.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Picture extensions the OCR reads. SVG is not among them: the product reads raster pictures
/// only, so drawings must be rasterised before they can be tested.
const PICTURE_EXTENSIONS: [&str; 6] = ["png", "jpg", "jpeg", "webp", "gif", "bmp"];

/// Pictures under `folder`, recursively, sorted by path.
pub fn discover(folder: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect(folder, &mut out);
    out.sort();
    out
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| PICTURE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str())) {
            out.push(path);
        }
    }
}

/// What a person marked on a picture. Rectangles are in picture pixels: `[x0, y0, x1, y1]`, origin
/// at the top left, with `x0 <= x1` and `y0 <= y1`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Label {
    pub text: String,
    pub rect: [f32; 4],
    /// The colour of this label's box, when the person picked one. Not set means the default blue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colour: Option<[u8; 3]>,
}

/// The label file beside a picture: `<picture>.label.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LabelFile {
    pub version: u32,
    /// The picture's file name, so a moved pair is still recognisable.
    pub picture: String,
    pub width: u32,
    pub height: u32,
    pub labels: Vec<Label>,
}

/// Languages the navigator groups by. A picture's folder names decide: the first keyword found in
/// its path wins. Anything else goes under "Other".
const LANGUAGES: [(&str, &str); 9] = [
    ("english", "English"),
    ("arabic", "Arabic"),
    ("chinese", "Chinese"),
    ("japanese", "Japanese"),
    ("korean", "Korean"),
    ("turkish", "Turkish"),
    ("french", "French"),
    ("german", "German"),
    ("hindi", "Hindi"),
];

/// Where a picture sits in the navigator: its kind (the first folder under the test folder, such as
/// handwriting, drawings or scans) and its language (from the folder names, see `LANGUAGES`).
pub fn group_of(folder: &Path, picture: &Path) -> (String, String) {
    let relative = picture.strip_prefix(folder).unwrap_or(picture);
    let parts: Vec<String> = relative.iter().map(|p| p.to_string_lossy().to_ascii_lowercase()).collect();
    let kind = if parts.len() > 1 { title(&parts[0]) } else { "Loose pictures".to_string() };
    // The folders decide, not the file's own name (see `LANGUAGES`).
    let folders = &parts[..parts.len().saturating_sub(1)];
    let language = folders
        .iter()
        .find_map(|part| LANGUAGES.iter().find(|(key, _)| part.contains(key)).map(|(_, name)| name.to_string()))
        .unwrap_or_else(|| "Other".to_string());
    (kind, language)
}

fn title(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub fn label_path(picture: &Path) -> PathBuf {
    let name = picture.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    picture.with_file_name(format!("{name}.label.json"))
}

/// The saved OCR run beside a picture: `<picture>.ocr-run.json`.
pub fn run_path(picture: &Path) -> PathBuf {
    let name = picture.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    picture.with_file_name(format!("{name}.ocr-run.json"))
}

/// The SHA-256 of a picture's bytes, so a saved run can tell it still belongs to the picture.
pub fn picture_sha256(picture: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(picture).ok()?;
    Some(format!("{:x}", Sha256::digest(&bytes)))
}

/// Loads the labels of a picture. A picture with no label file gets an empty set.
pub fn load_labels(picture: &Path) -> Result<LabelFile, String> {
    let (width, height) = image::image_dimensions(picture).map_err(|e| format!("{}: {e}", picture.display()))?;
    let name = picture.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let path = label_path(picture);
    match std::fs::read_to_string(&path) {
        // Files written on Windows can start with a byte-order mark, which JSON does not allow.
        Ok(text) => serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(LabelFile { version: 1, picture: name, width, height, labels: Vec::new() }),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Saves the labels through a temporary file, so a crash leaves the old file, never half of one.
pub fn save_labels(picture: &Path, file: &LabelFile) -> Result<(), String> {
    let path = label_path(picture);
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, format!("{text}\n")).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The axis-aligned bounds of an engine line's four corners, as `[x0, y0, x1, y1]`.
pub fn bounds(quad: &[notecraft_ocr::Point; 4]) -> [f32; 4] {
    let xs = quad.iter().map(|p| p.x);
    let ys = quad.iter().map(|p| p.y);
    [xs.clone().fold(f32::MAX, f32::min), ys.clone().fold(f32::MAX, f32::min), xs.fold(f32::MIN, f32::max), ys.fold(f32::MIN, f32::max)]
}

/// Intersection over union of two `[x0, y0, x1, y1]` rectangles, in `0.0..=1.0`.
pub fn iou(a: [f32; 4], b: [f32; 4]) -> f32 {
    let w = (a[2].min(b[2]) - a[0].max(b[0])).max(0.0);
    let h = (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
    let inter = w * h;
    let area = |r: [f32; 4]| (r[2] - r[0]).max(0.0) * (r[3] - r[1]).max(0.0);
    let union = area(a) + area(b) - inter;
    if union <= 0.0 { 0.0 } else { inter / union }
}

/// Two rectangles count as the same line when they overlap this much.
pub const MATCH_IOU: f32 = 0.5;

/// Text compared loosely: case and runs of spaces do not count.
pub fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// How one run of the engine compares with a label set.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Score {
    pub labels: usize,
    /// Labels with an engine line over them (IoU at least `MATCH_IOU`).
    pub found: usize,
    /// Found labels whose text equals the engine's text, loosely.
    pub text_matched: usize,
    pub engine_lines: usize,
    /// Engine lines that no label covers: candidate false positives.
    pub unmatched_engine_lines: usize,
}

pub fn score(labels: &[Label], lines: &[notecraft_ocr::Line]) -> Score {
    let boxes: Vec<[f32; 4]> = lines.iter().map(|l| bounds(&l.quad)).collect();
    let mut covered = vec![false; lines.len()];
    let mut found = 0;
    let mut text_matched = 0;
    for label in labels {
        let best = boxes.iter().enumerate().map(|(i, b)| (i, iou(label.rect, *b))).max_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, _)) = best.filter(|(_, o)| *o >= MATCH_IOU) {
            found += 1;
            covered[i] = true;
            if !label.text.trim().is_empty() && normalize(&label.text) == normalize(&lines[i].text) {
                text_matched += 1;
            }
        }
    }
    // A line counts as covered when any label overlaps it, even one that lost the best-match race.
    for (i, b) in boxes.iter().enumerate() {
        if labels.iter().any(|l| iou(l.rect, *b) >= MATCH_IOU) {
            covered[i] = true;
        }
    }
    Score { labels: labels.len(), found, text_matched, engine_lines: lines.len(), unmatched_engine_lines: covered.iter().filter(|c| !**c).count() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_ocr::{Line, Point};

    fn line(text: &str, x0: f32, y0: f32, x1: f32, y1: f32) -> Line {
        let quad = [Point { x: x0, y: y0 }, Point { x: x1, y: y0 }, Point { x: x1, y: y1 }, Point { x: x0, y: y1 }];
        Line::new(text, quad, None).expect("a line")
    }

    #[test]
    fn a_picture_is_grouped_by_its_folder_kind_and_language() {
        let folder = Path::new("/cases");
        let (kind, language) = group_of(folder, Path::new("/cases/handwriting/english-gnhk/p1.png"));
        assert_eq!((kind.as_str(), language.as_str()), ("Handwriting", "English"));
        let (kind, language) = group_of(folder, Path::new("/cases/drawings/mermaid-diagrams/a.png"));
        assert_eq!((kind.as_str(), language.as_str()), ("Drawings", "Other"));
        let (kind, language) = group_of(folder, Path::new("/cases/handwriting/scans/english-sample.png"));
        assert_eq!((kind.as_str(), language.as_str()), ("Handwriting", "Other"), "a file's own name does not pick the language");
        let (kind, _) = group_of(folder, Path::new("/cases/loose.png"));
        assert_eq!(kind, "Loose pictures");
    }

    #[test]
    fn overlap_is_intersection_over_union() {
        assert_eq!(iou([0.0, 0.0, 10.0, 10.0], [0.0, 0.0, 10.0, 10.0]), 1.0);
        assert_eq!(iou([0.0, 0.0, 10.0, 10.0], [20.0, 20.0, 30.0, 30.0]), 0.0);
        assert!((iou([0.0, 0.0, 10.0, 10.0], [0.0, 0.0, 10.0, 5.0]) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_label_over_a_line_is_found_and_its_text_compared_loosely() {
        let labels = vec![Label { text: "Sourdough  6.00".into(), rect: [10.0, 10.0, 110.0, 30.0], colour: None }];
        let lines = vec![line("sourdough 6.00", 10.0, 10.0, 110.0, 30.0)];
        let s = score(&labels, &lines);
        assert_eq!((s.labels, s.found, s.text_matched, s.unmatched_engine_lines), (1, 1, 1, 0));
    }

    #[test]
    fn a_label_with_no_line_over_it_is_not_found_and_an_uncovered_line_is_counted() {
        let labels = vec![Label { text: "Flat white".into(), rect: [10.0, 50.0, 110.0, 70.0], colour: None }];
        let lines = vec![line("Sourdough", 10.0, 10.0, 110.0, 30.0)];
        let s = score(&labels, &lines);
        assert_eq!((s.found, s.text_matched, s.unmatched_engine_lines), (0, 0, 1));
    }

    #[test]
    fn a_found_label_with_different_text_is_found_but_not_text_matched() {
        let labels = vec![Label { text: "Flat white".into(), rect: [10.0, 10.0, 110.0, 30.0], colour: None }];
        let lines = vec![line("Flat whlte", 10.0, 10.0, 110.0, 30.0)];
        let s = score(&labels, &lines);
        assert_eq!((s.found, s.text_matched), (1, 0));
    }

    #[test]
    fn a_label_file_with_a_byte_order_mark_still_loads() {
        let dir = std::env::temp_dir().join(format!("ocr-lab-bom-{}", crate::unique_suffix()));
        std::fs::create_dir_all(&dir).expect("dir");
        let picture = dir.join("q.png");
        image::RgbImage::new(4, 4).save(&picture).expect("picture");
        let text = "\u{feff}{\"version\":1,\"picture\":\"q.png\",\"width\":4,\"height\":4,\"labels\":[]}";
        std::fs::write(label_path(&picture), text).expect("file");
        assert!(load_labels(&picture).expect("loads").labels.is_empty());
        std::fs::remove_dir_all(&dir).expect("clean up");
    }

    #[test]
    fn a_label_file_round_trips_beside_its_picture() {
        let dir = std::env::temp_dir().join(format!("ocr-lab-case-{}", crate::unique_suffix()));
        std::fs::create_dir_all(&dir).expect("dir");
        let picture = dir.join("p.png");
        image::RgbImage::new(20, 10).save(&picture).expect("picture");
        let mut file = load_labels(&picture).expect("no label file yet");
        assert!(file.labels.is_empty());
        assert_eq!((file.width, file.height), (20, 10));
        file.labels.push(Label { text: "hi".into(), rect: [1.0, 2.0, 3.0, 4.0], colour: None });
        save_labels(&picture, &file).expect("save");
        assert_eq!(load_labels(&picture).expect("load"), file);
        std::fs::remove_dir_all(&dir).expect("clean up");
    }
}

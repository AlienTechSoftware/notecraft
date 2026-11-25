//! Adding a test picture from inside the lab (issue #489): a picture file, or a drawing made on the
//! lab's canvas, put into one group of the test cases.
//!
//! A picture goes to one of two places. By default it stays on this computer, under
//! `local/inserted/` (git-ignored), so nothing is added to the repository by accident. A person can
//! instead put it into the test folder beside the group's other pictures; then its source and
//! licence, which the person gives, are written to that folder's `SOURCES.txt`, as for every other
//! test picture. Labels and runs are kept beside the picture either way, as for any picture.

use std::path::{Path, PathBuf};

/// The picture files that can be inserted. The lab reads more kinds, but a test picture is a photo
/// or a scan, and these two are what cameras and scanners write.
pub const EXTENSIONS: [&str; 3] = ["png", "jpg", "jpeg"];

/// Where a picture inserted for this computer only is kept, for the test folder `folder`: its own
/// folder under `root` (the lab's `local/inserted`), so each test folder shows its own.
pub fn local_folder(root: &Path, folder: &Path) -> PathBuf {
    let name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "tests".to_string());
    root.join(safe_name(&name))
}

/// The folder of a group, for a picture kept on this computer: `<kind>/<language>`, as the lab
/// groups by folder names (see `case::group_of`).
pub fn local_group_folder(local: &Path, kind: &str, language: &str) -> PathBuf {
    local.join(safe_name(&kind.to_ascii_lowercase())).join(safe_name(&language.to_ascii_lowercase()))
}

/// A name with only letters, digits, `-` and `_`, so it is a file name on every system.
pub fn safe_name(name: &str) -> String {
    let cleaned: String = name.trim().chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() { "picture".to_string() } else { trimmed.to_string() }
}

/// What is inserted.
pub enum Picture {
    /// A picture file, copied as it is.
    File(PathBuf),
    /// A drawing from the canvas, written as a PNG.
    Drawn(image::RgbaImage),
}

/// Where the picture came from and its licence, as written to `SOURCES.txt`.
pub struct Record {
    pub source: String,
    pub licence: String,
}

/// Puts `picture` into `dir` as `<name>.<ext>`, with `-2`, `-3` … added when the name is taken, and
/// returns its path. With a `record`, appends the picture's line to `SOURCES.txt` in `dir`. A file
/// that is not a PNG or JPEG picture, or that cannot be read as one, is refused before anything is
/// written.
pub fn insert(dir: &Path, name: &str, picture: &Picture, record: Option<&Record>) -> Result<PathBuf, String> {
    if let Some(record) = record
        && (record.source.trim().is_empty() || record.licence.trim().is_empty())
    {
        return Err("A picture added to the test folder needs its source and licence.".to_string());
    }
    let extension = match picture {
        Picture::File(path) => {
            let extension = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
            if !EXTENSIONS.contains(&extension.as_str()) {
                return Err(format!("{} is not a PNG or JPEG picture.", file_name(path)));
            }
            image::image_dimensions(path).map_err(|e| format!("{} cannot be read as a picture: {e}", file_name(path)))?;
            extension
        }
        Picture::Drawn(_) => "png".to_string(),
    };
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let stem = safe_name(name);
    let target = (1..)
        .map(|n| if n == 1 { dir.join(format!("{stem}.{extension}")) } else { dir.join(format!("{stem}-{n}.{extension}")) })
        .find(|path| !path.exists())
        .unwrap_or_else(|| dir.join(format!("{stem}.{extension}")));
    match picture {
        Picture::File(path) => {
            std::fs::copy(path, &target).map_err(|e| format!("{}: {e}", target.display()))?;
        }
        Picture::Drawn(pixels) => pixels.save(&target).map_err(|e| format!("{}: {e}", target.display()))?,
    }
    if let Some(record) = record {
        let sources = dir.join("SOURCES.txt");
        let before = std::fs::read_to_string(&sources).unwrap_or_default();
        let gap = if before.is_empty() || before.ends_with('\n') { "" } else { "\n" };
        let line = format!("{gap}{}: {}. Licence: {}.\n", file_name(&target), one_line(&record.source), one_line(&record.licence));
        std::fs::write(&sources, before + &line).map_err(|e| format!("{}: {e}", sources.display()))?;
    }
    Ok(target)
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").trim_end_matches('.').to_string()
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The drawing canvas: pen strokes on white paper, in canvas pixels.
pub struct Sketch {
    pub size: [u32; 2],
    pub strokes: Vec<Vec<[f32; 2]>>,
    /// The pen's width, in canvas pixels.
    pub width: f32,
}

impl Sketch {
    pub fn new(size: [u32; 2]) -> Sketch {
        Sketch { size, strokes: Vec::new(), width: 6.0 }
    }

    pub fn is_empty(&self) -> bool {
        self.strokes.iter().all(Vec::is_empty)
    }

    /// The drawing as a picture: black strokes on white, round-ended, as the pen drew them.
    pub fn raster(&self) -> image::RgbaImage {
        let [w, h] = self.size;
        let mut out = image::RgbaImage::from_pixel(w, h, image::Rgba([255, 255, 255, 255]));
        let radius = (self.width / 2.0).max(0.5);
        for stroke in &self.strokes {
            let segments: Vec<([f32; 2], [f32; 2])> =
                if stroke.len() == 1 { vec![(stroke[0], stroke[0])] } else { stroke.windows(2).map(|p| (p[0], p[1])).collect() };
            for (a, b) in segments {
                let (x0, x1) = (a[0].min(b[0]) - radius, a[0].max(b[0]) + radius);
                let (y0, y1) = (a[1].min(b[1]) - radius, a[1].max(b[1]) + radius);
                for y in (y0.floor().max(0.0) as u32)..=(y1.ceil().min(h as f32 - 1.0).max(0.0) as u32) {
                    for x in (x0.floor().max(0.0) as u32)..=(x1.ceil().min(w as f32 - 1.0).max(0.0) as u32) {
                        let p = [x as f32 + 0.5, y as f32 + 0.5];
                        if distance_to_segment(p, a, b) <= radius {
                            out.put_pixel(x, y, image::Rgba([0, 0, 0, 255]));
                        }
                    }
                }
            }
        }
        out
    }
}

fn distance_to_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = dx * dx + dy * dy;
    let t = if length == 0.0 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length).clamp(0.0, 1.0) };
    let (cx, cy) = (a[0] + t * dx, a[1] + t * dy);
    ((p[0] - cx).powi(2) + (p[1] - cy).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ocr-lab-insert-{name}-{}", crate::unique_suffix()));
        std::fs::create_dir_all(&dir).expect("dir");
        dir
    }

    fn png(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        image::RgbaImage::from_pixel(4, 3, image::Rgba([10, 20, 30, 255])).save(&path).expect("png");
        path
    }

    #[test]
    fn a_file_is_copied_under_a_free_name_and_its_source_is_recorded() {
        let from = temp("from");
        let group = temp("group");
        let picture = png(&from, "Shopping list.PNG");
        let record = Record { source: "Photographed by the tester\nfor this test".into(), licence: "CC0".into() };
        let first = insert(&group, "Shopping list", &Picture::File(picture.clone()), Some(&record)).expect("first");
        let second = insert(&group, "Shopping list", &Picture::File(picture), Some(&record)).expect("second");
        assert_eq!(first, group.join("Shopping-list.png"));
        assert_eq!(second, group.join("Shopping-list-2.png"), "a taken name is not written over");
        assert_eq!(image::image_dimensions(&second).expect("a picture"), (4, 3));
        let sources = std::fs::read_to_string(group.join("SOURCES.txt")).expect("sources");
        assert_eq!(
            sources,
            "Shopping-list.png: Photographed by the tester for this test. Licence: CC0.\nShopping-list-2.png: Photographed by the tester for this test. Licence: CC0.\n"
        );
        let _ = std::fs::remove_dir_all(from);
        let _ = std::fs::remove_dir_all(group);
    }

    #[test]
    fn a_picture_kept_on_this_computer_writes_no_sources_line() {
        let local = temp("local");
        let folder = PathBuf::from("/tests/mvp-smoke");
        let dir = local_group_folder(&local_folder(&local, &folder), "Handwriting", "English");
        assert_eq!(dir, local.join("mvp-smoke").join("handwriting").join("english"));
        let mut sketch = Sketch::new([40, 20]);
        sketch.strokes.push(vec![[5.0, 10.0], [35.0, 10.0]]);
        let path = insert(&dir, "my drawing", &Picture::Drawn(sketch.raster()), None).expect("inserted");
        assert_eq!(path, dir.join("my-drawing.png"));
        assert!(!dir.join("SOURCES.txt").exists());
        // The lab groups it under the group it was inserted into.
        assert_eq!(crate::case::group_of(&local_folder(&local, &folder), &path), ("Handwriting".to_string(), "English".to_string()));
        let _ = std::fs::remove_dir_all(local);
    }

    #[test]
    fn what_is_not_a_picture_or_lacks_a_licence_is_refused_and_nothing_is_written() {
        let from = temp("bad-from");
        let group = temp("bad-group");
        let text = from.join("notes.txt");
        std::fs::write(&text, "words").expect("text");
        assert!(insert(&group, "notes", &Picture::File(text), None).is_err());
        let fake = from.join("fake.png");
        std::fs::write(&fake, "not a png").expect("fake");
        assert!(insert(&group, "fake", &Picture::File(fake), None).is_err());
        let picture = png(&from, "real.png");
        let record = Record { source: "Mine".into(), licence: " ".into() };
        assert!(insert(&group, "real", &Picture::File(picture), Some(&record)).is_err());
        assert_eq!(std::fs::read_dir(&group).expect("group").count(), 0, "nothing was written");
        let _ = std::fs::remove_dir_all(from);
        let _ = std::fs::remove_dir_all(group);
    }

    #[test]
    fn a_drawing_is_black_where_the_pen_went_and_white_elsewhere() {
        let mut sketch = Sketch::new([30, 30]);
        assert!(sketch.is_empty());
        sketch.strokes.push(vec![[5.0, 15.0], [25.0, 15.0]]);
        sketch.strokes.push(vec![[15.0, 5.0]]);
        let pixels = sketch.raster();
        assert_eq!(pixels.dimensions(), (30, 30));
        assert_eq!(pixels.get_pixel(15, 15).0, [0, 0, 0, 255], "on the line");
        assert_eq!(pixels.get_pixel(15, 5).0, [0, 0, 0, 255], "a dot");
        assert_eq!(pixels.get_pixel(15, 25).0, [255, 255, 255, 255], "off the line");
        assert_eq!(pixels.get_pixel(1, 15).0, [255, 255, 255, 255], "past the round end");
    }

    #[test]
    fn names_become_plain_file_names() {
        assert_eq!(safe_name("  ../a b?.png "), "a-b--png");
        assert_eq!(safe_name("***"), "picture");
        assert_eq!(safe_name("مرحبا"), "مرحبا");
    }
}

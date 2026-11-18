//! Insert > Stickers (issue #286): a small gallery of pictures to put on a page.
//!
//! Every sticker is NoteCraft's own drawing, made for this project as plain SVG shapes with no
//! text and no fonts, and licensed with the code (see `ATTRIBUTION.md`). A sticker put on a
//! page is copied into the page's own folder, as any picture is, so the page owns it in both
//! formats and it opens anywhere; the gallery is only where it was chosen from.

/// One sticker: the name its file is given on the page, its accessible name in English (as
/// the catalog has it, so `tl!` translates it), and its picture.
pub struct Sticker {
    pub id: &'static str,
    pub label: &'static str,
    pub svg: &'static [u8],
}

macro_rules! sticker {
    ($id:literal, $label:literal) => {
        Sticker { id: $id, label: $label, svg: include_bytes!(concat!("../../../assets/stickers/", $id, ".svg")) }
    };
}

/// The gallery, in the order it is shown.
pub const ALL: [Sticker; 12] = [
    sticker!("star", "Star sticker"),
    sticker!("heart", "Heart sticker"),
    sticker!("check", "Check mark sticker"),
    sticker!("smile", "Smiling face sticker"),
    sticker!("sun", "Sun sticker"),
    sticker!("lightbulb", "Light bulb sticker"),
    sticker!("warning", "Warning sticker"),
    sticker!("question", "Question sticker"),
    sticker!("flower", "Flower sticker"),
    sticker!("trophy", "Trophy sticker"),
    sticker!("rainbow", "Rainbow sticker"),
    sticker!("pin", "Pin sticker"),
];

/// The sticker called `id`.
pub fn find(id: &str) -> Option<&'static Sticker> {
    ALL.iter().find(|s| s.id == id)
}

/// How wide a sticker is put on the page, in points: big enough to see, small enough to sit
/// beside the words. It can be resized like any picture.
pub const WIDTH: f32 = 96.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sticker_is_an_svg_picture_with_no_text_in_it() {
        for s in &ALL {
            let text = std::str::from_utf8(s.svg).expect("utf-8");
            assert!(text.starts_with("<svg") && text.contains("viewBox=\"0 0 128 128\""), "{}", s.id);
            assert!(!text.contains("<text") && !text.contains("<image") && !text.contains("href"), "{}: shapes only", s.id);
            assert!(find(s.id).is_some());
        }
        let mut ids: Vec<&str> = ALL.iter().map(|s| s.id).collect();
        ids.dedup();
        assert_eq!(ids.len(), ALL.len(), "every name once");
    }
}
